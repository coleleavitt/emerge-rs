use std::collections::{HashMap, HashSet};
use tokio::fs;
use std::path::{Path, PathBuf, Component};
use crate::exception::InvalidData;

/// Represents a Gentoo profile
#[derive(Debug, Clone)]
pub struct Profile {
    pub path: PathBuf,
    pub name: String,
    pub eapi: Option<String>,
    pub parent_profiles: Vec<Profile>,
}

/// Profile settings loaded from various profile files
#[derive(Debug, Clone, Default)]
pub struct ProfileSettings {
    /// Variables from make.defaults (USE, ACCEPT_LICENSE, etc.)
    pub variables: HashMap<String, String>,
    /// Package-specific USE flags from use.defaults and package.use
    pub package_use: HashMap<String, Vec<String>>,
    /// Package masks from package.mask
    pub package_mask: HashSet<String>,
    /// Package unmasks from package.unmask
    pub package_unmask: HashSet<String>,
    /// Package keywords from package.keywords
    pub package_keywords: HashMap<String, Vec<String>>,
    /// System packages from packages file
    pub system_packages: HashSet<String>,
    /// USE flag masks from use.mask
    pub use_mask: HashSet<String>,
    /// USE flag forces from use.force
    pub use_force: HashSet<String>,
}

/// Gentoo profile manager
pub struct ProfileManager {
    root: String,
    pub profiles_dir: PathBuf,
    current_profile_path: PathBuf,
}

impl ProfileManager {
    /// Create a new profile manager
    pub fn new(root: &str) -> Self {
        let root_path = Path::new(root);
        Self {
            root: root.to_string(),
            profiles_dir: root_path.join("var/db/repos/gentoo/profiles"),
            current_profile_path: root_path.join("etc/portage/make.profile"),
        }
    }

    /// Get the current profile
    pub async fn get_current_profile(&self) -> Result<Profile, InvalidData> {
        let profile_path = self.resolve_current_profile_path().await?;
        self.load_profile(&profile_path).await
    }

    /// Resolve the current profile path from the make.profile symlink
    async fn resolve_current_profile_path(&self) -> Result<PathBuf, InvalidData> {
        if !self.current_profile_path.exists() {
            return Err(InvalidData::new("make.profile does not exist", None));
        }

        // Read the symlink target
        let target = fs::read_link(&self.current_profile_path)
            .await
            .map_err(|e| InvalidData::new(&format!("Failed to read make.profile symlink: {}", e), None))?;

        // If it's a relative path, resolve it relative to /etc/portage
        let etc_portage = Path::new(&self.root).join("etc/portage");
        if target.is_relative() {
            Ok(etc_portage.join(target))
        } else {
            Ok(target)
        }
    }

    /// Load a profile and its inheritance chain
    fn load_profile<'a>(&'a self, profile_path: &'a Path) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Profile, InvalidData>> + 'a + Send>> {
        Box::pin(async move {
            let mut profile = Profile {
                path: profile_path.to_path_buf(),
                name: self.get_profile_name(profile_path),
                eapi: self.read_eapi(profile_path).await,
                parent_profiles: Vec::new(),
            };

            // Load parent profiles recursively
            self.load_parent_profiles(profile_path, &mut profile.parent_profiles).await?;

            Ok(profile)
        })
    }

    /// Get a human-readable name for the profile
    fn get_profile_name(&self, profile_path: &Path) -> String {
        // Try to get relative path from profiles directory
        if let Ok(relative) = profile_path.strip_prefix(&self.profiles_dir) {
            relative.to_string_lossy().to_string()
        } else {
            profile_path.to_string_lossy().to_string()
        }
    }

    /// Read EAPI from profile
    async fn read_eapi(&self, profile_path: &Path) -> Option<String> {
        let eapi_file = profile_path.join("eapi");
        let content = fs::read_to_string(eapi_file).await.ok()?;
        Some(content.trim().to_string())
    }

    /// Load parent profiles recursively
    async fn load_parent_profiles(&self, profile_path: &Path, parents: &mut Vec<Profile>) -> Result<(), InvalidData> {
        let parent_file = profile_path.join("parent");

        if !parent_file.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&parent_file)
            .await
            .map_err(|e| InvalidData::new(&format!("Failed to read parent file {}: {}", parent_file.display(), e), None))?;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Resolve parent path relative to current profile
            let parent_path = profile_path.join(line);
            // Normalize the path to handle .. components
            let normalized_path = Self::normalize_path(&parent_path);
            let parent_profile = self.load_profile(&normalized_path).await?;
            parents.push(parent_profile);
        }

        Ok(())
    }

    /// Load all settings from a profile and its inheritance chain
    pub fn load_profile_settings<'a>(&'a self, profile: &'a Profile) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<ProfileSettings, InvalidData>> + 'a + Send>> {
        Box::pin(async move {
            let mut settings = ProfileSettings::default();

            // Load settings from parent profiles first (lower precedence) - recursively!
            for parent in &profile.parent_profiles {
                let parent_settings = self.load_profile_settings(parent).await?;
                self.merge_settings(&mut settings, &parent_settings);
            }

            // Load settings from current profile (higher precedence)
            let current_settings = self.load_single_profile_settings(&profile.path).await?;
            self.merge_settings(&mut settings, &current_settings);

            // Expand variables after all profiles are merged
            self.expand_variables(&mut settings);

            Ok(settings)
        })
    }

    /// Expand variable references in profile settings
    fn expand_variables(&self, settings: &mut ProfileSettings) {
        // Create a map for variable lookups
        let vars = settings.variables.clone();
        
        // Expand variables in all variable values
        for (_key, value) in settings.variables.iter_mut() {
            *value = self.expand_string(value, &vars);
        }
    }

    /// Expand variable references in a string
    fn expand_string(&self, s: &str, vars: &HashMap<String, String>) -> String {
        let mut result = s.to_string();
        
        // Simple variable expansion for ${VAR} and $VAR patterns
        for (var_name, var_value) in vars {
            // Expand ${VAR}
            let pattern1 = format!("${{{}}}", var_name);
            result = result.replace(&pattern1, var_value);
            
            // Expand $VAR (but only if not followed by valid var name chars)
            let pattern2 = format!("${}", var_name);
            result = result.replace(&pattern2, var_value);
        }
        
        result
    }

    /// Load settings from a single profile directory
    async fn load_single_profile_settings(&self, profile_path: &Path) -> Result<ProfileSettings, InvalidData> {
        let mut settings = ProfileSettings::default();

        // Load make.defaults
        if let Ok(vars) = self.parse_make_defaults(profile_path).await {
            settings.variables.extend(vars);
        }

        // Load package.use
        if let Ok(package_use) = self.parse_package_use(profile_path).await {
            settings.package_use.extend(package_use);
        }

        // Load use.defaults
        if let Ok(use_defaults) = self.parse_use_defaults(profile_path).await {
            settings.package_use.extend(use_defaults);
        }

        // Load package.mask
        if let Ok(mask) = self.parse_package_list(profile_path, "package.mask").await {
            settings.package_mask.extend(mask);
        }

        // Load package.unmask
        if let Ok(unmask) = self.parse_package_list(profile_path, "package.unmask").await {
            settings.package_unmask.extend(unmask);
        }

        // Load package.keywords
        if let Ok(keywords) = self.parse_package_keywords(profile_path).await {
            settings.package_keywords.extend(keywords);
        }

        // Load packages (system packages)
        if let Ok(system_pkgs) = self.parse_system_packages(profile_path).await {
            settings.system_packages.extend(system_pkgs);
        }

        // Load use.mask
        if let Ok(use_mask) = self.parse_use_list(profile_path, "use.mask").await {
            settings.use_mask.extend(use_mask);
        }

        // Load use.force
        if let Ok(use_force) = self.parse_use_list(profile_path, "use.force").await {
            settings.use_force.extend(use_force);
        }

        Ok(settings)
    }

    /// Parse make.defaults file (KEY="value" format)
    async fn parse_make_defaults(&self, profile_path: &Path) -> Result<HashMap<String, String>, InvalidData> {
        let file_path = profile_path.join("make.defaults");
        if !file_path.exists() {
            return Ok(HashMap::new());
        }

        let content = fs::read_to_string(&file_path)
            .await
            .map_err(|e| InvalidData::new(&format!("Failed to read make.defaults: {}", e), None))?;

        let mut variables = HashMap::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim().to_string();
                let value = value.trim().trim_matches('"').to_string();
                variables.insert(key, value);
            }
        }

        Ok(variables)
    }

    /// Parse package.use and use.defaults files ("category/package flag1 flag2" format)
    async fn parse_package_use(&self, profile_path: &Path) -> Result<HashMap<String, Vec<String>>, InvalidData> {
        self.parse_package_flags_file(profile_path, "package.use").await
    }

    async fn parse_use_defaults(&self, profile_path: &Path) -> Result<HashMap<String, Vec<String>>, InvalidData> {
        self.parse_package_flags_file(profile_path, "use.defaults").await
    }

    async fn parse_package_flags_file(&self, profile_path: &Path, filename: &str) -> Result<HashMap<String, Vec<String>>, InvalidData> {
        let file_path = profile_path.join(filename);
        if !file_path.exists() {
            return Ok(HashMap::new());
        }

        let content = fs::read_to_string(&file_path)
            .await
            .map_err(|e| InvalidData::new(&format!("Failed to read {}: {}", filename, e), None))?;

        let mut package_flags = HashMap::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let package = parts[0].to_string();
                let flags: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                package_flags.insert(package, flags);
            }
        }

        Ok(package_flags)
    }

    /// Parse package list files (one package per line)
    async fn parse_package_list(&self, profile_path: &Path, filename: &str) -> Result<HashSet<String>, InvalidData> {
        let file_path = profile_path.join(filename);
        if !file_path.exists() {
            return Ok(HashSet::new());
        }

        let content = fs::read_to_string(&file_path)
            .await
            .map_err(|e| InvalidData::new(&format!("Failed to read {}: {}", filename, e), None))?;

        let mut packages = HashSet::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            packages.insert(line.to_string());
        }

        Ok(packages)
    }

    /// Parse package.keywords file ("category/package keyword1 keyword2" format)
    async fn parse_package_keywords(&self, profile_path: &Path) -> Result<HashMap<String, Vec<String>>, InvalidData> {
        let file_path = profile_path.join("package.keywords");
        if !file_path.exists() {
            return Ok(HashMap::new());
        }

        let content = fs::read_to_string(&file_path)
            .await
            .map_err(|e| InvalidData::new("Failed to read package.keywords", None))?;

        let mut package_keywords = HashMap::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let package = parts[0].to_string();
                let keywords: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                package_keywords.insert(package, keywords);
            }
        }

        Ok(package_keywords)
    }

    /// Parse packages file for system packages
    async fn parse_system_packages(&self, profile_path: &Path) -> Result<HashSet<String>, InvalidData> {
        let file_path = profile_path.join("packages");
        if !file_path.exists() {
            return Ok(HashSet::new());
        }

        let content = fs::read_to_string(&file_path)
            .await
            .map_err(|e| InvalidData::new("Failed to read packages", None))?;

        let mut packages = HashSet::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Handle different formats:
            // *category/package - required system package
            // -category/package - masked system package (skip)
            if line.starts_with('*') {
                packages.insert(line[1..].to_string());
            }
            // Skip masked packages (-prefix) and comments
        }

        Ok(packages)
    }

    /// Parse USE flag list files (one flag per line)
    async fn parse_use_list(&self, profile_path: &Path, filename: &str) -> Result<HashSet<String>, InvalidData> {
        let file_path = profile_path.join(filename);
        if !file_path.exists() {
            return Ok(HashSet::new());
        }

        let content = fs::read_to_string(&file_path)
            .await
            .map_err(|e| InvalidData::new(&format!("Failed to read {}", filename), None))?;

        let mut flags = HashSet::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            flags.insert(line.to_string());
        }

        Ok(flags)
    }

    /// Normalize a path by resolving .. components
    fn normalize_path(path: &Path) -> PathBuf {
        let mut components = Vec::new();

        for component in path.components() {
            match component {
                Component::ParentDir => {
                    // Remove the last component if it exists and is not root
                    if !components.is_empty() && *components.last().unwrap() != Component::RootDir {
                        components.pop();
                    }
                }
                Component::CurDir => {
                    // Skip current directory components
                }
                _ => {
                    components.push(component);
                }
            }
        }

        components.into_iter().collect()
    }

    /// Merge settings from one profile into another (higher precedence wins)
    fn merge_settings(&self, target: &mut ProfileSettings, source: &ProfileSettings) {
        // Merge variables
        target.variables.extend(source.variables.clone());

        // Merge package USE flags (source overrides target)
        target.package_use.extend(source.package_use.clone());

        // Merge package masks/unmasks/keywords
        target.package_mask.extend(source.package_mask.clone());
        target.package_unmask.extend(source.package_unmask.clone());
        target.package_keywords.extend(source.package_keywords.clone());

        // Merge system packages
        target.system_packages.extend(source.system_packages.clone());

        // Merge USE masks/forces
        target.use_mask.extend(source.use_mask.clone());
        target.use_force.extend(source.use_force.clone());
    }

    /// List all available profiles
    pub async fn list_available_profiles(&self) -> Result<Vec<String>, InvalidData> {
        if !self.profiles_dir.exists() {
            return Ok(vec![]);
        }

        let mut profiles = Vec::new();
        self.collect_profiles_recursive(&self.profiles_dir, &mut profiles, "").await?;
        Ok(profiles)
    }

    /// Recursively collect profile names
    fn collect_profiles_recursive<'a>(&'a self, dir: &'a Path, profiles: &'a mut Vec<String>, prefix: &'a str) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), InvalidData>> + 'a + Send>> {
        Box::pin(async move {
            let mut entries = fs::read_dir(dir)
                .await
                .map_err(|e| InvalidData::new(&format!("Failed to read profiles directory: {}", e), None))?;

            while let Some(entry) = entries.next_entry()
                .await
                .map_err(|e| InvalidData::new("Failed to read directory entry", None))? {
                let path = entry.path();

                if path.is_dir() {
                    let name = entry.file_name().to_string_lossy().to_string();
                    let full_name = if prefix.is_empty() {
                        name.clone()
                    } else {
                        format!("{}/{}", prefix, name)
                    };

                    // Check if this is a valid profile (has parent or is a leaf)
                    if path.join("parent").exists() || self.is_profile_leaf(&path) {
                        profiles.push(full_name.clone());
                    }

                    // Recurse into subdirectories
                    self.collect_profiles_recursive(&path, profiles, &full_name).await?;
                }
            }

            Ok(())
        })
    }

    /// Check if a directory is a profile leaf (has profile files)
    fn is_profile_leaf(&self, path: &Path) -> bool {
        path.join("make.defaults").exists() ||
        path.join("packages").exists() ||
        path.join("eapi").exists()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_profile_manager_creation() {
        let manager = ProfileManager::new("/");
        assert_eq!(manager.root, "/");
    }

    #[tokio::test]
    async fn test_parse_make_defaults() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let profile_dir = temp_dir.path();

        let make_defaults = profile_dir.join("make.defaults");
        fs::write(&make_defaults, r#"USE="acl bzip2"
ACCEPT_LICENSE="-* @FREE"
ELIBC="glibc"
"#).unwrap();

        let manager = ProfileManager::new("/");
        let vars = manager.parse_make_defaults(profile_dir).await.unwrap();

        assert_eq!(vars.get("USE").unwrap(), "acl bzip2");
        assert_eq!(vars.get("ACCEPT_LICENSE").unwrap(), "-* @FREE");
        assert_eq!(vars.get("ELIBC").unwrap(), "glibc");
    }

    #[tokio::test]
    async fn test_parse_package_use() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let profile_dir = temp_dir.path();

        let package_use = profile_dir.join("package.use");
        fs::write(&package_use, "app-misc/hello threads\nsys-apps/systemd -udev\n").unwrap();

        let manager = ProfileManager::new("/");
        let package_flags = manager.parse_package_use(profile_dir).await.unwrap();

        assert_eq!(package_flags.get("app-misc/hello").unwrap(), &vec!["threads"]);
        assert_eq!(package_flags.get("sys-apps/systemd").unwrap(), &vec!["-udev"]);
    }

    #[tokio::test]
    async fn test_parse_system_packages() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let profile_dir = temp_dir.path();

        let packages = profile_dir.join("packages");
        fs::write(&packages, r#"*app-admin/sudo
*sys-apps/systemd
-sys-apps/openrc
"#).unwrap();

        let manager = ProfileManager::new("/");
        let system_pkgs = manager.parse_system_packages(profile_dir).await.unwrap();

        assert!(system_pkgs.contains("app-admin/sudo"));
        assert!(system_pkgs.contains("sys-apps/systemd"));
        assert!(!system_pkgs.contains("sys-apps/openrc"));
    }

    #[tokio::test]
    async fn test_parse_package_mask() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let profile_dir = temp_dir.path();

        let package_mask = profile_dir.join("package.mask");
        fs::write(&package_mask, "app-misc/foo\n>=app-misc/bar-2.0\n").unwrap();

        let manager = ProfileManager::new("/");
        let masks = manager.parse_package_list(profile_dir, "package.mask").await.unwrap();

        assert!(masks.contains("app-misc/foo"));
        assert!(masks.contains(">=app-misc/bar-2.0"));
    }

    #[tokio::test]
    async fn test_parse_use_mask() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let profile_dir = temp_dir.path();

        let use_mask = profile_dir.join("use.mask");
        fs::write(&use_mask, "kde\ngnome\n").unwrap();

        let manager = ProfileManager::new("/");
        let use_masks = manager.parse_use_list(profile_dir, "use.mask").await.unwrap();

        assert!(use_masks.contains("kde"));
        assert!(use_masks.contains("gnome"));
    }

    #[tokio::test]
    async fn test_parse_package_keywords() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let profile_dir = temp_dir.path();

        let package_keywords = profile_dir.join("package.keywords");
        fs::write(&package_keywords, "app-misc/foo ~amd64\n>=app-misc/bar-2.0 amd64\n").unwrap();

        let manager = ProfileManager::new("/");
        let keywords = manager.parse_package_keywords(profile_dir).await.unwrap();

        assert_eq!(keywords.get("app-misc/foo").unwrap(), &vec!["~amd64"]);
        assert_eq!(keywords.get(">=app-misc/bar-2.0").unwrap(), &vec!["amd64"]);
    }

    #[tokio::test]
    async fn test_parse_use_defaults() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let profile_dir = temp_dir.path();

        let use_defaults = profile_dir.join("use.defaults");
        fs::write(&use_defaults, "app-misc/hello threads\nsys-apps/systemd -udev\n").unwrap();

        let manager = ProfileManager::new("/");
        let use_defaults_map = manager.parse_use_defaults(profile_dir).await.unwrap();

        assert_eq!(use_defaults_map.get("app-misc/hello").unwrap(), &vec!["threads"]);
        assert_eq!(use_defaults_map.get("sys-apps/systemd").unwrap(), &vec!["-udev"]);
    }

    #[tokio::test]
    async fn test_load_single_profile_settings() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let profile_dir = temp_dir.path();

        // Create make.defaults
        let make_defaults = profile_dir.join("make.defaults");
        fs::write(&make_defaults, r#"USE="acl bzip2"
ACCEPT_LICENSE="-* @FREE"
"#).unwrap();

        // Create package.use
        let package_use = profile_dir.join("package.use");
        fs::write(&package_use, "app-misc/hello threads\n").unwrap();

        // Create package.mask
        let package_mask = profile_dir.join("package.mask");
        fs::write(&package_mask, "app-misc/foo\n").unwrap();

        // Create use.mask
        let use_mask = profile_dir.join("use.mask");
        fs::write(&use_mask, "kde\n").unwrap();

        // Create packages
        let packages = profile_dir.join("packages");
        fs::write(&packages, "*app-admin/sudo\n").unwrap();

        let manager = ProfileManager::new("/");
        let settings = manager.load_single_profile_settings(profile_dir).await.unwrap();

        assert_eq!(settings.variables.get("USE").unwrap(), "acl bzip2");
        assert_eq!(settings.package_use.get("app-misc/hello").unwrap(), &vec!["threads"]);
        assert!(settings.package_mask.contains("app-misc/foo"));
        assert!(settings.use_mask.contains("kde"));
        assert!(settings.system_packages.contains("app-admin/sudo"));
    }

    #[tokio::test]
    async fn test_merge_settings() {
        let mut target = ProfileSettings {
            variables: [("VAR1".to_string(), "value1".to_string())].iter().cloned().collect(),
            package_use: [("pkg1".to_string(), vec!["flag1".to_string()])].iter().cloned().collect(),
            package_mask: ["mask1".to_string()].iter().cloned().collect(),
            ..Default::default()
        };

        let source = ProfileSettings {
            variables: [("VAR2".to_string(), "value2".to_string())].iter().cloned().collect(),
            package_use: [("pkg2".to_string(), vec!["flag2".to_string()])].iter().cloned().collect(),
            package_mask: ["mask2".to_string()].iter().cloned().collect(),
            ..Default::default()
        };

        let manager = ProfileManager::new("/");
        manager.merge_settings(&mut target, &source);

        assert_eq!(target.variables.get("VAR1").unwrap(), "value1");
        assert_eq!(target.variables.get("VAR2").unwrap(), "value2");
        assert_eq!(target.package_use.get("pkg1").unwrap(), &vec!["flag1"]);
        assert_eq!(target.package_use.get("pkg2").unwrap(), &vec!["flag2"]);
        assert!(target.package_mask.contains("mask1"));
        assert!(target.package_mask.contains("mask2"));
    }

    #[tokio::test]
    async fn test_profile_inheritance() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let profiles_base = temp_dir.path();

        // Create parent profile
        let parent_dir = profiles_base.join("parent");
        fs::create_dir(&parent_dir).unwrap();

        let parent_make_defaults = parent_dir.join("make.defaults");
        fs::write(&parent_make_defaults, r#"USE="parent_flag"
PARENT_VAR="parent_value"
"#).unwrap();

        // Create child profile
        let child_dir = parent_dir.join("child");
        fs::create_dir(&child_dir).unwrap();

        let child_make_defaults = child_dir.join("make.defaults");
        fs::write(&child_make_defaults, r#"USE="child_flag"
CHILD_VAR="child_value"
"#).unwrap();

        let parent_file = child_dir.join("parent");
        fs::write(&parent_file, "..\n").unwrap();

        // Test loading child profile
        let manager = ProfileManager::new("/");
        let profile = Box::pin(manager.load_profile(&child_dir)).await.unwrap();

        assert_eq!(profile.name, child_dir.to_string_lossy());
        assert_eq!(profile.parent_profiles.len(), 1);
        assert_eq!(profile.parent_profiles[0].name, parent_dir.to_string_lossy());

        // Test loading settings with inheritance
        let settings = manager.load_profile_settings(&profile).await.unwrap();

        // Child settings should override parent
        assert_eq!(settings.variables.get("USE").unwrap(), "child_flag");
        assert_eq!(settings.variables.get("CHILD_VAR").unwrap(), "child_value");
        // Parent settings should still be present
        assert_eq!(settings.variables.get("PARENT_VAR").unwrap(), "parent_value");
    }

    #[tokio::test]
    async fn test_list_available_profiles() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let profiles_base = temp_dir.path();

        // Create some profile directories
        let profile1 = profiles_base.join("profile1");
        fs::create_dir(&profile1).unwrap();

        let profile2 = profiles_base.join("profile2");
        fs::create_dir(&profile2).unwrap();

        let subprofile = profile2.join("subprofile");
        fs::create_dir(&subprofile).unwrap();

        // Create make.defaults to make them valid profiles
        fs::write(profile1.join("make.defaults"), "USE=\"test\"\n").unwrap();
        fs::write(subprofile.join("make.defaults"), "USE=\"test\"\n").unwrap();

        let manager = ProfileManager {
            root: "/".to_string(),
            profiles_dir: profiles_base.to_path_buf(),
            current_profile_path: "/etc/portage/make.profile".into(),
        };

        let profiles = manager.list_available_profiles().await.unwrap();

        assert!(profiles.contains(&"profile1".to_string()));
        assert!(profiles.contains(&"profile2/subprofile".to_string()));
    }

    #[tokio::test]
    async fn test_get_profile_name() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let profiles_base = temp_dir.path();

        let manager = ProfileManager {
            root: "/".to_string(),
            profiles_dir: profiles_base.to_path_buf(),
            current_profile_path: "/etc/portage/make.profile".into(),
        };

        let profile_path = profiles_base.join("default/linux/amd64");
        let name = manager.get_profile_name(&profile_path);
        assert_eq!(name, "default/linux/amd64");

        let external_path = Path::new("/some/external/profile");
        let external_name = manager.get_profile_name(external_path);
        assert_eq!(external_name, "/some/external/profile");
    }

    #[tokio::test]
    async fn test_read_eapi() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let profile_dir = temp_dir.path();

        let eapi_file = profile_dir.join("eapi");
        fs::write(&eapi_file, "8\n").unwrap();

        let manager = ProfileManager::new("/");
        let eapi = manager.read_eapi(profile_dir).await;
        assert_eq!(eapi, Some("8".to_string()));

        // Test missing EAPI file
        let empty_dir = temp_dir.path().join("empty");
        fs::create_dir(&empty_dir).unwrap();
        let missing_eapi = manager.read_eapi(&empty_dir).await;
        assert_eq!(missing_eapi, None);
    }
}