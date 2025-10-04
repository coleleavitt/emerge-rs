use std::fs;
use std::path::{Path, PathBuf};
use crate::exception::InvalidData;
use crate::profile::ProfileManager;

/// Package set types
#[derive(Debug, Clone, PartialEq)]
pub enum PackageSet {
    World,
    System,
    Selected,
    Profile,
    Custom(String),
}

/// Information about a package set
#[derive(Debug, Clone)]
pub struct SetInfo {
    pub name: String,
    pub description: String,
    pub package_count: usize,
}

/// Selected packages tracking
#[derive(Debug)]
pub struct SelectedPackages {
    root: String,
}

impl SelectedPackages {
    pub fn new(root: &str) -> Self {
        Self {
            root: root.to_string(),
        }
    }

    /// Get the path to the selected packages file
    fn selected_file(&self) -> PathBuf {
        Path::new(&self.root).join("var/lib/portage/selected")
    }

    /// Get packages in @selected set
    pub fn get_selected_packages(&self) -> Result<Vec<String>, InvalidData> {
        let selected_file = self.selected_file();
        if !selected_file.exists() {
            return Ok(vec![]);
        }

        let content = fs::read_to_string(&selected_file)
            .map_err(|e| InvalidData::new(&format!("Failed to read selected file: {}", e), None))?;

        Ok(content.lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && !s.starts_with('#'))
            .collect())
    }

    /// Add packages to @selected set
    pub fn add_selected_packages(&self, packages: &[String]) -> Result<(), InvalidData> {
        let selected_file = self.selected_file();

        // Create directory if it doesn't exist
        if let Some(parent) = selected_file.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| InvalidData::new(&format!("Failed to create selected directory: {}", e), None))?;
        }

        let mut existing = self.get_selected_packages()?;

        for pkg in packages {
            if !existing.contains(pkg) {
                existing.push(pkg.clone());
            }
        }

        // Sort for consistency
        existing.sort();

        let content = existing.join("\n") + "\n";
        fs::write(&selected_file, content)
            .map_err(|e| InvalidData::new(&format!("Failed to write selected file: {}", e), None))?;

        Ok(())
    }

    /// Remove packages from @selected set
    pub fn remove_selected_packages(&self, packages: &[String]) -> Result<(), InvalidData> {
        let mut existing = self.get_selected_packages()?;
        existing.retain(|pkg| !packages.contains(pkg));

        let selected_file = self.selected_file();
        let content = existing.join("\n") + "\n";
        fs::write(&selected_file, content)
            .map_err(|e| InvalidData::new(&format!("Failed to write selected file: {}", e), None))?;

        Ok(())
    }
}

/// Package set manager for handling @world, @system, etc.
pub struct PackageSetManager {
    root: String,
    sets_dir: PathBuf,
    profile_manager: ProfileManager,
    selected_manager: SelectedPackages,
}

impl PackageSetManager {
    pub fn new(root: &str) -> Self {
        let root_path = Path::new(root);
        Self {
            root: root.to_string(),
            sets_dir: root_path.join("etc/portage/sets"),
            profile_manager: ProfileManager::new(root),
            selected_manager: SelectedPackages::new(root),
        }
    }

    /// Resolve a set name to a list of package atoms
    pub async fn resolve_set(&self, set_name: &str) -> Result<Vec<String>, InvalidData> {
        match set_name {
            "world" => self.get_world_packages(),
            "system" => self.get_system_packages().await,
            "selected" => self.selected_manager.get_selected_packages(),
            "profile" => self.get_profile_packages().await,
            custom => self.get_custom_set(custom),
        }
    }

    /// Get packages in @world set
    pub fn get_world_packages(&self) -> Result<Vec<String>, InvalidData> {
        let world_file = Path::new(&self.root).join("var/lib/portage/world");
        if !world_file.exists() {
            return Ok(vec![]);
        }

        let content = fs::read_to_string(&world_file)
            .map_err(|e| InvalidData::new(&format!("Failed to read world file: {}", e), None))?;

        Ok(content.lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty() && !s.starts_with('#'))
            .collect())
    }

    /// Add packages to @world set
    pub fn add_to_world(&self, packages: &[String]) -> Result<(), InvalidData> {
        let world_file = Path::new(&self.root).join("var/lib/portage/world");

        // Create directory if it doesn't exist
        if let Some(parent) = world_file.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| InvalidData::new(&format!("Failed to create world directory: {}", e), None))?;
        }

        let mut existing = self.get_world_packages()?;

        for pkg in packages {
            if !existing.contains(pkg) {
                existing.push(pkg.clone());
                // When adding to world, also add to selected if not already there
                let _ = self.selected_manager.add_selected_packages(&[pkg.clone()]);
            }
        }

        // Sort for consistency
        existing.sort();

        let content = existing.join("\n") + "\n";
        fs::write(&world_file, content)
            .map_err(|e| InvalidData::new(&format!("Failed to write world file: {}", e), None))?;

        Ok(())
    }

    /// Remove packages from @world set
    pub fn remove_from_world(&self, packages: &[String]) -> Result<(), InvalidData> {
        let mut existing = self.get_world_packages()?;
        existing.retain(|pkg| !packages.contains(pkg));

        let world_file = Path::new(&self.root).join("var/lib/portage/world");
        let content = existing.join("\n") + "\n";
        fs::write(&world_file, content)
            .map_err(|e| InvalidData::new(&format!("Failed to write world file: {}", e), None))?;

        Ok(())
    }

    /// Add packages to @selected set
    pub fn add_to_selected(&self, packages: &[String]) -> Result<(), InvalidData> {
        self.selected_manager.add_selected_packages(packages)
    }

    /// Remove packages from @selected set
    pub fn remove_from_selected(&self, packages: &[String]) -> Result<(), InvalidData> {
        self.selected_manager.remove_selected_packages(packages)
    }

    /// Get packages in @system set
    pub async fn get_system_packages(&self) -> Result<Vec<String>, InvalidData> {
        let mut all_packages = Vec::new();

        // Get current profile and its inheritance chain
        if let Ok(current_profile) = self.profile_manager.get_current_profile().await {
            // Check profiles in inheritance order (parent first, then child)
            let mut profiles = current_profile.parent_profiles.clone();
            profiles.push(current_profile);

            for profile in profiles {
                let packages_file = profile.path.join("packages");

                if packages_file.exists() {
                    let content = fs::read_to_string(&packages_file)
                        .map_err(|e| InvalidData::new(&format!("Failed to read profile packages {}: {}", packages_file.display(), e), None))?;

                    let profile_packages = self.parse_packages_file(&content)?;

                    // Add packages from this profile (child profiles override parents)
                    for pkg in profile_packages {
                        // Remove any existing entry for this package (allows overriding)
                        all_packages.retain(|p| p != &pkg);
                        all_packages.push(pkg);
                    }
                }
            }
        }

        Ok(all_packages)
    }

    /// Parse a packages file content (only required packages with *)
    fn parse_packages_file(&self, content: &str) -> Result<Vec<String>, InvalidData> {
        let mut packages = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Handle different formats:
            // *category/package - required system package
            // -category/package - masked system package (skip)
            if line.starts_with('*') {
                packages.push(line[1..].to_string());
            }
            // Skip masked packages (-prefix) and comments
        }

        Ok(packages)
    }

    /// Parse a packages file content allowing both required (*) and optional packages
    fn parse_packages_file_allow_optional(&self, content: &str) -> Result<Vec<String>, InvalidData> {
        let mut packages = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Handle different formats:
            // *category/package - required system package
            // category/package - optional package (for @profile set)
            // -category/package - masked system package (skip)
            if line.starts_with('*') {
                packages.push(line[1..].to_string());
            } else if !line.starts_with('-') && line.contains('/') {
                // Optional package (no prefix, contains /)
                packages.push(line.to_string());
            }
            // Skip masked packages (-prefix) and comments
        }

        Ok(packages)
    }

    /// Get packages in @profile set
    pub async fn get_profile_packages(&self) -> Result<Vec<String>, InvalidData> {
        let mut all_packages = Vec::new();

        // Get current profile and its inheritance chain
        if let Ok(current_profile) = self.profile_manager.get_current_profile().await {
            // Check profiles in inheritance order (parent first, then child)
            let mut profiles = current_profile.parent_profiles.clone();
            profiles.push(current_profile);

            for profile in profiles {
                let packages_file = profile.path.join("packages");

                if packages_file.exists() {
                    let content = fs::read_to_string(&packages_file)
                        .map_err(|e| InvalidData::new(&format!("Failed to read profile packages {}: {}", packages_file.display(), e), None))?;

                    let profile_packages = self.parse_packages_file_allow_optional(&content)?;

                    // Add packages from this profile (child profiles override parents)
                    for pkg in profile_packages {
                        // Remove any existing entry for this package (allows overriding)
                        all_packages.retain(|p| p != &pkg);
                        all_packages.push(pkg);
                    }
                }
            }
        }

        Ok(all_packages)
    }

    /// Get packages from a custom user-defined set
    pub fn get_custom_set(&self, set_name: &str) -> Result<Vec<String>, InvalidData> {
        let set_file = self.sets_dir.join(set_name);

        if !set_file.exists() {
            return Err(InvalidData::new(&format!("Custom set '{}' not found", set_name), None));
        }

        let content = fs::read_to_string(&set_file)
            .map_err(|e| InvalidData::new(&format!("Failed to read custom set '{}': {}", set_name, e), None))?;

        let mut packages = Vec::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            packages.push(line.to_string());
        }

        Ok(packages)
    }

    /// Create a custom set
    pub fn create_custom_set(&self, set_name: &str, packages: &[String]) -> Result<(), InvalidData> {
        // Create sets directory if it doesn't exist
        fs::create_dir_all(&self.sets_dir)
            .map_err(|e| InvalidData::new(&format!("Failed to create sets directory: {}", e), None))?;

        let set_file = self.sets_dir.join(set_name);
        let content = packages.join("\n") + "\n";

        fs::write(&set_file, content)
            .map_err(|e| InvalidData::new(&format!("Failed to write custom set '{}': {}", set_name, e), None))?;

        Ok(())
    }

    /// List available custom sets
    pub fn list_custom_sets(&self) -> Result<Vec<String>, InvalidData> {
        if !self.sets_dir.exists() {
            return Ok(vec![]);
        }

        let mut sets = Vec::new();

        for entry in fs::read_dir(&self.sets_dir)
            .map_err(|e| InvalidData::new(&format!("Failed to read sets directory: {}", e), None))? {

            let entry = entry
                .map_err(|e| InvalidData::new(&format!("Failed to read directory entry: {}", e), None))?;

            if let Some(name) = entry.file_name().to_str() {
                sets.push(name.to_string());
            }
        }

        Ok(sets)
    }

    /// List all available sets (built-in + custom)
    pub fn list_all_sets(&self) -> Result<Vec<String>, InvalidData> {
        let mut sets = vec![
            "world".to_string(),
            "system".to_string(),
            "selected".to_string(),
            "profile".to_string(),
        ];

        // Add custom sets
        sets.extend(self.list_custom_sets()?);

        Ok(sets)
    }

    /// Check if a set exists
    pub fn set_exists(&self, set_name: &str) -> bool {
        match set_name {
            "world" | "system" | "selected" | "profile" => true,
            custom => self.sets_dir.join(custom).exists(),
        }
    }

    /// Get set information (description, etc.)
    pub async fn get_set_info(&self, set_name: &str) -> Result<SetInfo, InvalidData> {
        let packages = self.resolve_set(set_name).await?;

        let description = match set_name {
            "world" => "All user-installed packages",
            "system" => "Essential system packages required for basic operation",
            "selected" => "Packages explicitly selected for installation",
            "profile" => "Packages defined in the current profile",
            _ => "Custom user-defined package set",
        };

        Ok(SetInfo {
            name: set_name.to_string(),
            description: description.to_string(),
            package_count: packages.len(),
        })
    }
}

/// Resolve targets that may include sets (prefixed with @)
pub async fn resolve_targets(targets: &[String], root: &str) -> Result<Vec<String>, InvalidData> {
    let set_manager = PackageSetManager::new(root);
    let mut resolved = Vec::new();

    for target in targets {
        if target.starts_with('@') {
            // It's a set
            let set_name = &target[1..];
            let packages = set_manager.resolve_set(set_name).await?;
            resolved.extend(packages);
        } else {
            // Regular package
            resolved.push(target.clone());
        }
    }

    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_world_packages() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let set_manager = PackageSetManager::new(temp_path);

        // Test empty world
        assert_eq!(set_manager.get_world_packages().unwrap(), Vec::<String>::new());

        // Add packages to world
        set_manager.add_to_world(&["app-misc/hello".to_string(), "dev-lang/rust".to_string()]).unwrap();

        let world_packages = set_manager.get_world_packages().unwrap();
        assert_eq!(world_packages.len(), 2);
        assert!(world_packages.contains(&"app-misc/hello".to_string()));
        assert!(world_packages.contains(&"dev-lang/rust".to_string()));
    }

    #[tokio::test]
    async fn test_custom_sets() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let set_manager = PackageSetManager::new(temp_path);

        // Create a custom set
        let packages = vec!["xfce-base/xfce4-meta".to_string(), "xfce-extra/xfce4-terminal".to_string()];
        set_manager.create_custom_set("xfce-desktop", &packages).unwrap();

        // Read it back
        let read_packages = set_manager.get_custom_set("xfce-desktop").unwrap();
        assert_eq!(read_packages, packages);

        // List custom sets
        let sets = set_manager.list_custom_sets().unwrap();
        assert_eq!(sets, vec!["xfce-desktop".to_string()]);
    }

    #[tokio::test]
    async fn test_resolve_targets() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let set_manager = PackageSetManager::new(temp_path);

        // Create a custom set
        set_manager.create_custom_set("test-set", &["pkg1".to_string(), "pkg2".to_string()]).unwrap();

        // Resolve mixed targets
        let targets = vec!["regular-pkg".to_string(), "@test-set".to_string()];
        let resolved = resolve_targets(&targets, temp_path).await.unwrap();

        assert_eq!(resolved, vec!["regular-pkg", "pkg1", "pkg2"]);
    }

    #[tokio::test]
    async fn test_selected_packages() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let set_manager = PackageSetManager::new(temp_path);

        // Test empty selected
        assert_eq!(set_manager.resolve_set("selected").await.unwrap(), Vec::<String>::new());

        // Add packages to selected
        set_manager.add_to_selected(&["app-misc/hello".to_string(), "dev-lang/rust".to_string()]).unwrap();

        let selected_packages = set_manager.resolve_set("selected").await.unwrap();
        assert_eq!(selected_packages.len(), 2);
        assert!(selected_packages.contains(&"app-misc/hello".to_string()));
        assert!(selected_packages.contains(&"dev-lang/rust".to_string()));

        // Remove package from selected
        set_manager.remove_from_selected(&["dev-lang/rust".to_string()]).unwrap();
        let selected_packages = set_manager.resolve_set("selected").await.unwrap();
        assert_eq!(selected_packages.len(), 1);
        assert!(selected_packages.contains(&"app-misc/hello".to_string()));
    }

    #[tokio::test]
    async fn test_list_all_sets() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let set_manager = PackageSetManager::new(temp_path);

        // Create a custom set
        set_manager.create_custom_set("my-set", &["pkg1".to_string()]).unwrap();

        let all_sets = set_manager.list_all_sets().unwrap();

        assert!(all_sets.contains(&"world".to_string()));
        assert!(all_sets.contains(&"system".to_string()));
        assert!(all_sets.contains(&"selected".to_string()));
        assert!(all_sets.contains(&"profile".to_string()));
        assert!(all_sets.contains(&"my-set".to_string()));
    }

    #[tokio::test]
    async fn test_set_exists() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let set_manager = PackageSetManager::new(temp_path);

        // Create a custom set
        set_manager.create_custom_set("test-set", &["pkg1".to_string()]).unwrap();

        assert!(set_manager.set_exists("world"));
        assert!(set_manager.set_exists("system"));
        assert!(set_manager.set_exists("selected"));
        assert!(set_manager.set_exists("profile"));
        assert!(set_manager.set_exists("test-set"));
        assert!(!set_manager.set_exists("nonexistent"));
    }

    #[tokio::test]
    async fn test_parse_packages_file_required() {
        let set_manager = PackageSetManager::new("/");
        let content = "*sys-apps/baselayout\n*sys-libs/glibc\n#comment\n-sys-apps/foo\n";
        let packages = set_manager.parse_packages_file(content).unwrap();

        assert_eq!(packages.len(), 2);
        assert!(packages.contains(&"sys-apps/baselayout".to_string()));
        assert!(packages.contains(&"sys-libs/glibc".to_string()));
    }

    #[tokio::test]
    async fn test_parse_packages_file_allow_optional() {
        let set_manager = PackageSetManager::new("/");
        let content = "*sys-apps/baselayout\napp-misc/hello\n#comment\n-sys-apps/foo\n";
        let packages = set_manager.parse_packages_file_allow_optional(content).unwrap();

        assert_eq!(packages.len(), 2);
        assert!(packages.contains(&"sys-apps/baselayout".to_string()));
        assert!(packages.contains(&"app-misc/hello".to_string()));
    }

    #[tokio::test]
    async fn test_system_packages() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        // Create a mock profile structure
        let profiles_dir = temp_dir.path().join("var/db/repos/gentoo/profiles");
        fs::create_dir_all(&profiles_dir).unwrap();

        let base_profile_dir = profiles_dir.join("base");
        fs::create_dir(&base_profile_dir).unwrap();

        // Create packages file for base profile
        let base_packages = base_profile_dir.join("packages");
        fs::write(&base_packages, "*sys-apps/baselayout\n*sys-libs/glibc\n").unwrap();

        let desktop_profile_dir = profiles_dir.join("desktop");
        fs::create_dir(&desktop_profile_dir).unwrap();

        // Create parent file pointing to base
        let parent_file = desktop_profile_dir.join("parent");
        fs::write(&parent_file, "../base\n").unwrap();

        // Create packages file for desktop profile (overrides baselayout)
        let desktop_packages = desktop_profile_dir.join("packages");
        fs::write(&desktop_packages, "*sys-apps/baselayout\n*app-misc/hello\n").unwrap();

        // Create make.profile symlink
        let etc_portage = temp_dir.path().join("etc/portage");
        fs::create_dir_all(&etc_portage).unwrap();
        let make_profile = etc_portage.join("make.profile");
        unix_fs::symlink(&desktop_profile_dir, &make_profile).unwrap();

        let set_manager = PackageSetManager::new(temp_path);
        let system_packages = set_manager.get_system_packages().await.unwrap();

        // Should contain packages from both profiles, with desktop overriding base
        assert_eq!(system_packages.len(), 3);
        assert!(system_packages.contains(&"sys-apps/baselayout".to_string()));
        assert!(system_packages.contains(&"sys-libs/glibc".to_string()));
        assert!(system_packages.contains(&"app-misc/hello".to_string()));
    }

    #[tokio::test]
    async fn test_profile_packages() {
        let temp_dir = TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        // Create a mock profile structure
        let profiles_dir = temp_dir.path().join("var/db/repos/gentoo/profiles");
        fs::create_dir_all(&profiles_dir).unwrap();

        let base_profile_dir = profiles_dir.join("base");
        fs::create_dir(&base_profile_dir).unwrap();

        // Create packages file for base profile with required and optional
        let base_packages = base_profile_dir.join("packages");
        fs::write(&base_packages, "*sys-apps/baselayout\napp-misc/editor\n").unwrap();

        let desktop_profile_dir = profiles_dir.join("desktop");
        fs::create_dir(&desktop_profile_dir).unwrap();

        // Create parent file pointing to base
        let parent_file = desktop_profile_dir.join("parent");
        fs::write(&parent_file, "../base\n").unwrap();

        // Create packages file for desktop profile
        let desktop_packages = desktop_profile_dir.join("packages");
        fs::write(&desktop_packages, "*sys-apps/baselayout\napp-misc/terminal\n").unwrap();

        // Create make.profile symlink
        let etc_portage = temp_dir.path().join("etc/portage");
        fs::create_dir_all(&etc_portage).unwrap();
        let make_profile = etc_portage.join("make.profile");
        unix_fs::symlink(&desktop_profile_dir, &make_profile).unwrap();

        let set_manager = PackageSetManager::new(temp_path);
        let profile_packages = set_manager.get_profile_packages().await.unwrap();

        // Should contain all packages from both profiles
        assert_eq!(profile_packages.len(), 3);
        assert!(profile_packages.contains(&"sys-apps/baselayout".to_string()));
        assert!(profile_packages.contains(&"app-misc/editor".to_string()));
        assert!(profile_packages.contains(&"app-misc/terminal".to_string()));
    }
}