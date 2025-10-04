// config.rs - Configuration handling

use std::collections::{HashMap, HashSet};
use tokio::fs;
use std::path::{Path, PathBuf};
use crate::exception::InvalidData;
use crate::profile::{ProfileManager, ProfileSettings};

#[derive(Debug)]
pub struct Config {
    pub root: String,
    pub make_conf: HashMap<String, String>,
    pub profile_settings: ProfileSettings,
    pub use_flags: Vec<String>,
    pub accept_keywords: Vec<String>,
    pub features: Vec<String>,
    // User configuration files (override profile settings)
    pub package_use: HashMap<String, Vec<String>>,
    pub package_keywords: HashMap<String, Vec<String>>,
    pub package_mask: HashSet<String>,
    pub package_unmask: HashSet<String>,
    pub sets_conf: HashMap<String, Vec<String>>,
    // Binary package repository (binhost) configuration
    pub binhost: Vec<String>, // List of binhost URIs
    pub binhost_mirrors: Vec<String>, // Additional binhost mirrors
}

impl Config {
    pub async fn new(root: &str) -> Result<Self, InvalidData> {
        let mut config = Config {
            root: root.to_string(),
            make_conf: HashMap::new(),
            profile_settings: ProfileSettings::default(),
            use_flags: vec![],
            accept_keywords: vec![],
            features: vec![],
            package_use: HashMap::new(),
            package_keywords: HashMap::new(),
            package_mask: HashSet::new(),
            package_unmask: HashSet::new(),
            sets_conf: HashMap::new(),
            binhost: vec![],
            binhost_mirrors: vec![],
        };

        // Load profile settings first (lower precedence)
        // If profile loading fails, use defaults (for testing or minimal setups)
        if let Err(_e) = config.load_profile_settings().await {
            config.profile_settings = ProfileSettings::default();
        }

        // Load make.conf (higher precedence, can override profile)
        config.load_make_conf().await?;

        // Parse FEATURES from make.conf
        config.parse_features();

        // Parse binhost configuration from make.conf
        config.parse_binhost_config();

        // Load user configuration files (highest precedence)
        config.load_package_use().await?;
        config.load_package_keywords().await?;
        config.load_package_mask().await?;
        config.load_package_unmask().await?;
        config.load_sets_conf().await?;

        // Parse USE flags from both sources
        config.parse_use_flags();

        // Parse ACCEPT_KEYWORDS from both sources
        config.parse_accept_keywords();

        Ok(config)
    }

    async fn load_make_conf(&mut self) -> Result<(), InvalidData> {
        let make_conf_path = Path::new(&self.root).join("etc/portage/make.conf");
        if make_conf_path.exists() {
            let content = fs::read_to_string(&make_conf_path)
                .await
                .map_err(|e| InvalidData::new(&format!("Failed to read make.conf: {}", e), None))?;
            Self::parse_config_file(&content, &mut self.make_conf);
        }
        Ok(())
    }

    async fn load_package_use(&mut self) -> Result<(), InvalidData> {
        let package_use_path = Path::new(&self.root).join("etc/portage/package.use");
        Self::load_package_config_files(package_use_path, &mut self.package_use).await
    }

    async fn load_package_keywords(&mut self) -> Result<(), InvalidData> {
        let package_keywords_path = Path::new(&self.root).join("etc/portage/package.keywords");
        Self::load_package_config_files(package_keywords_path, &mut self.package_keywords).await
    }

    async fn load_package_mask(&mut self) -> Result<(), InvalidData> {
        let package_mask_path = Path::new(&self.root).join("etc/portage/package.mask");
        Self::load_package_list_files(package_mask_path, &mut self.package_mask).await
    }

    async fn load_package_unmask(&mut self) -> Result<(), InvalidData> {
        let package_unmask_path = Path::new(&self.root).join("etc/portage/package.unmask");
        Self::load_package_list_files(package_unmask_path, &mut self.package_unmask).await
    }

    async fn load_sets_conf(&mut self) -> Result<(), InvalidData> {
        let sets_conf_path = Path::new(&self.root).join("etc/portage/sets.conf");
        if sets_conf_path.exists() {
            let content = fs::read_to_string(&sets_conf_path)
                .await
                .map_err(|e| InvalidData::new(&format!("Failed to read sets.conf: {}", e), None))?;
            Self::parse_package_list_config(&content, &mut self.sets_conf);
        }
        Ok(())
    }

    async fn load_profile_settings(&mut self) -> Result<(), InvalidData> {
        let profile_manager = ProfileManager::new(&self.root);
        let profile = profile_manager.get_current_profile().await
            .map_err(|e| InvalidData::new(&format!("Failed to load current profile: {}", e.value), None))?;

        self.profile_settings = profile_manager.load_profile_settings(&profile).await
            .map_err(|e| InvalidData::new(&format!("Failed to load profile settings: {}", e.value), None))?;

        Ok(())
    }

    fn parse_config_file(content: &str, map: &mut HashMap<String, String>) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some((key, value)) = line.split_once('=') {
                let key = key.trim().to_string();
                let value = value.trim().trim_matches('"').to_string();
                map.insert(key, value);
            }
        }
    }

    /// Load package configuration files (package.use, package.keywords style)
    /// Can be a single file or a directory of files
    async fn load_package_config_files(base_path: PathBuf, target: &mut HashMap<String, Vec<String>>) -> Result<(), InvalidData> {
        if !base_path.exists() {
            return Ok(());
        }

        let metadata = fs::metadata(&base_path).await
            .map_err(|e| InvalidData::new(&format!("Failed to read metadata: {}", e), None))?;

        if metadata.is_file() {
            // Single file
            let content = fs::read_to_string(&base_path)
                .await
                .map_err(|e| InvalidData::new(&format!("Failed to read {}: {}", base_path.display(), e), None))?;
            Self::parse_package_config(&content, target);
        } else if metadata.is_dir() {
            // Directory of files
            let mut entries = fs::read_dir(&base_path)
                .await
                .map_err(|e| InvalidData::new(&format!("Failed to read directory {}: {}", base_path.display(), e), None))?;
            
            while let Some(entry) = entries.next_entry()
                .await
                .map_err(|e| InvalidData::new(&format!("Failed to read directory entry: {}", e), None))? {
                let path = entry.path();
                let entry_metadata = fs::metadata(&path).await
                    .map_err(|e| InvalidData::new(&format!("Failed to read metadata: {}", e), None))?;
                if entry_metadata.is_file() {
                    let content = fs::read_to_string(&path)
                        .await
                        .map_err(|e| InvalidData::new(&format!("Failed to read {}: {}", path.display(), e), None))?;
                    Self::parse_package_config(&content, target);
                }
            }
        }
        Ok(())
    }

    /// Load package list files (package.mask, package.unmask style)
    /// Can be a single file or a directory of files
    async fn load_package_list_files(base_path: PathBuf, target: &mut HashSet<String>) -> Result<(), InvalidData> {
        if !base_path.exists() {
            return Ok(());
        }

        let metadata = fs::metadata(&base_path).await
            .map_err(|e| InvalidData::new(&format!("Failed to read metadata: {}", e), None))?;

        if metadata.is_file() {
            // Single file
            let content = fs::read_to_string(&base_path)
                .await
                .map_err(|e| InvalidData::new(&format!("Failed to read {}: {}", base_path.display(), e), None))?;
            Self::parse_package_list(&content, target);
        } else if metadata.is_dir() {
            // Directory of files
            let mut entries = fs::read_dir(&base_path)
                .await
                .map_err(|e| InvalidData::new(&format!("Failed to read directory {}: {}", base_path.display(), e), None))?;
            
            while let Some(entry) = entries.next_entry()
                .await
                .map_err(|e| InvalidData::new(&format!("Failed to read directory entry: {}", e), None))? {
                let path = entry.path();
                let entry_metadata = fs::metadata(&path).await
                    .map_err(|e| InvalidData::new(&format!("Failed to read metadata: {}", e), None))?;
                if entry_metadata.is_file() {
                    let content = fs::read_to_string(&path)
                        .await
                        .map_err(|e| InvalidData::new(&format!("Failed to read {}: {}", path.display(), e), None))?;
                    Self::parse_package_list(&content, target);
                }
            }
        }
        Ok(())
    }

    /// Parse package configuration content (package.use/package.keywords format)
    fn parse_package_config(content: &str, target: &mut HashMap<String, Vec<String>>) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let package = parts[0].to_string();
                let flags: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                target.insert(package, flags);
            }
        }
    }

    /// Parse package list content (package.mask/package.unmask format)
    fn parse_package_list(content: &str, target: &mut HashSet<String>) {
        for line in content.lines() {
            let line = line.trim();
            if !line.is_empty() && !line.starts_with('#') {
                target.insert(line.to_string());
            }
        }
    }

    /// Parse sets.conf content
    fn parse_package_list_config(content: &str, target: &mut HashMap<String, Vec<String>>) {
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let set_name = parts[0].to_string();
                let packages: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                target.insert(set_name, packages);
            }
        }
    }

    fn parse_use_flags(&mut self) {
        // Start with USE flags from profile (make.defaults)
        if let Some(use_str) = self.profile_settings.variables.get("USE") {
            self.use_flags = use_str.split_whitespace().map(|s| s.to_string()).collect();
        }

        // Add USE flags from make.conf (can override profile)
        if let Some(use_str) = self.make_conf.get("USE") {
            self.use_flags.extend(use_str.split_whitespace().map(|s| s.to_string()));
        }

        // Remove duplicates while preserving order
        let mut seen = std::collections::HashSet::new();
        self.use_flags.retain(|flag| seen.insert(flag.clone()));
    }

    fn parse_accept_keywords(&mut self) {
        // Start with ACCEPT_KEYWORDS from profile (make.defaults)
        if let Some(keywords_str) = self.profile_settings.variables.get("ACCEPT_KEYWORDS") {
            self.accept_keywords = keywords_str.split_whitespace().map(|s| s.to_string()).collect();
        }

        // Add ACCEPT_KEYWORDS from make.conf (can override profile)
        if let Some(keywords_str) = self.make_conf.get("ACCEPT_KEYWORDS") {
            self.accept_keywords.extend(keywords_str.split_whitespace().map(|s| s.to_string()));
        }

        // Remove duplicates while preserving order
        let mut seen = std::collections::HashSet::new();
        self.accept_keywords.retain(|keyword| seen.insert(keyword.clone()));
    }

    pub fn get_var(&self, key: &str) -> Option<&String> {
        self.make_conf.get(key).or_else(|| self.profile_settings.variables.get(key))
    }

    /// Get USE flags as a HashMap for dependency resolution
    pub fn get_use_flags_map(&self) -> std::collections::HashMap<String, bool> {
        let mut use_map = std::collections::HashMap::new();

        // Add all enabled USE flags
        for flag in &self.use_flags {
            if flag.starts_with('-') {
                // Disabled flag
                use_map.insert(flag[1..].to_string(), false);
            } else {
                // Enabled flag
                use_map.insert(flag.clone(), true);
            }
        }

        use_map
    }

    /// Get package-specific USE flags (user config overrides profile)
    pub fn get_package_use_flags(&self, package: &str) -> Option<&Vec<String>> {
        self.package_use.get(package).or_else(|| self.profile_settings.package_use.get(package))
    }

    /// Check if a package is masked (user config overrides profile)
    pub fn is_package_masked(&self, package: &str) -> bool {
        self.package_mask.contains(package) || self.profile_settings.package_mask.contains(package)
    }

    /// Check if a package is unmasked (user config overrides profile)
    pub fn is_package_unmasked(&self, package: &str) -> bool {
        self.package_unmask.contains(package) || self.profile_settings.package_unmask.contains(package)
    }

    /// Get package keywords (user config overrides profile)
    pub fn get_package_keywords(&self, package: &str) -> Option<&Vec<String>> {
        self.package_keywords.get(package).or_else(|| self.profile_settings.package_keywords.get(package))
    }

    /// Check if a USE flag is masked in the profile
    pub fn is_use_flag_masked(&self, flag: &str) -> bool {
        self.profile_settings.use_mask.contains(flag)
    }

    /// Check if a USE flag is forced in the profile
    pub fn is_use_flag_forced(&self, flag: &str) -> bool {
        self.profile_settings.use_force.contains(flag)
    }

    /// Get system packages from profile
    pub fn get_system_packages(&self) -> &std::collections::HashSet<String> {
        &self.profile_settings.system_packages
    }

    /// Get custom package sets from sets.conf
    pub fn get_custom_sets(&self) -> &HashMap<String, Vec<String>> {
        &self.sets_conf
    }

    /// Get packages in a custom set
    pub fn get_set_packages(&self, set_name: &str) -> Option<&Vec<String>> {
        self.sets_conf.get(set_name)
    }

    /// Parse FEATURES from make.conf
    fn parse_features(&mut self) {
        // Parse FEATURES from make.conf
        if let Some(features_str) = self.make_conf.get("FEATURES") {
            self.features = features_str.split_whitespace().map(|s| s.to_string()).collect();
        }

        // Add default features if none specified
        if self.features.is_empty() {
            // Add some reasonable defaults for Gentoo-like behavior
            self.features = vec!["sandbox".to_string(), "userpriv".to_string()];
        }
    }

    /// Parse binhost configuration from make.conf
    fn parse_binhost_config(&mut self) {
        // Parse PORTAGE_BINHOST
        if let Some(binhost_str) = self.make_conf.get("PORTAGE_BINHOST") {
            self.binhost = binhost_str.split_whitespace().map(|s| s.to_string()).collect();
        }

        // Parse PORTAGE_BINHOST_MIRRORS
        if let Some(mirrors_str) = self.make_conf.get("PORTAGE_BINHOST_MIRRORS") {
            self.binhost_mirrors = mirrors_str.split_whitespace().map(|s| s.to_string()).collect();
        }
    }

    /// Get CONFIG_PROTECT paths
    pub fn get_config_protect(&self) -> Vec<String> {
        // Start with default CONFIG_PROTECT paths
        let mut config_protect = vec![
            "/etc".to_string(),
            "/usr/share/config".to_string(),
        ];

        // Add CONFIG_PROTECT from profile
        if let Some(cp_str) = self.profile_settings.variables.get("CONFIG_PROTECT") {
            config_protect.extend(cp_str.split_whitespace().map(|s| s.to_string()));
        }

        // Add CONFIG_PROTECT from make.conf (can override profile)
        if let Some(cp_str) = self.make_conf.get("CONFIG_PROTECT") {
            config_protect.extend(cp_str.split_whitespace().map(|s| s.to_string()));
        }

        // Remove duplicates while preserving order
        let mut seen = std::collections::HashSet::new();
        config_protect.retain(|path| seen.insert(path.clone()));

        config_protect
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_parse_package_config() {
        let mut target = HashMap::new();
        let content = r#"
# Comment
app-editors/vim X gtk
sys-apps/util-linux -static
app-misc/foo bar baz
"#;

        Config::parse_package_config(content, &mut target);

        assert_eq!(target.get("app-editors/vim"), Some(&vec!["X".to_string(), "gtk".to_string()]));
        assert_eq!(target.get("sys-apps/util-linux"), Some(&vec!["-static".to_string()]));
        assert_eq!(target.get("app-misc/foo"), Some(&vec!["bar".to_string(), "baz".to_string()]));
    }

    #[tokio::test]
    async fn test_parse_package_list() {
        let mut target = HashSet::new();
        let content = r#"
# Comment
app-editors/vim
sys-apps/util-linux
# Another comment
app-misc/foo
"#;

        Config::parse_package_list(content, &mut target);

        assert!(target.contains("app-editors/vim"));
        assert!(target.contains("sys-apps/util-linux"));
        assert!(target.contains("app-misc/foo"));
        assert_eq!(target.len(), 3);
    }

    #[tokio::test]
    async fn test_parse_package_list_config() {
        let mut target = HashMap::new();
        let content = r#"
# Comment
my-set app-editors/vim sys-apps/util-linux
another-set app-misc/foo
"#;

        Config::parse_package_list_config(content, &mut target);

        assert_eq!(target.get("my-set"), Some(&vec!["app-editors/vim".to_string(), "sys-apps/util-linux".to_string()]));
        assert_eq!(target.get("another-set"), Some(&vec!["app-misc/foo".to_string()]));
    }

    #[tokio::test]
    async fn test_load_package_use_file() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_str().unwrap();

        // Create package.use file
        let package_use_dir = temp_dir.path().join("etc/portage/package.use");
        fs::create_dir_all(&package_use_dir).unwrap();
        fs::write(package_use_dir.join("test"), "app-editors/vim X gtk\nsys-apps/util-linux -static\n").unwrap();

        let config = Config::new(root).await.unwrap();

        let vim_flags = config.get_package_use_flags("app-editors/vim");
        assert_eq!(vim_flags, Some(&vec!["X".to_string(), "gtk".to_string()]));

        let util_flags = config.get_package_use_flags("sys-apps/util-linux");
        assert_eq!(util_flags, Some(&vec!["-static".to_string()]));
    }

    #[tokio::test]
    async fn test_load_package_mask_file() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_str().unwrap();

        // Create package.mask file
        let package_mask_dir = temp_dir.path().join("etc/portage/package.mask");
        fs::create_dir_all(&package_mask_dir).unwrap();
        fs::write(package_mask_dir.join("test"), "app-editors/vim\n# Comment\nsys-apps/util-linux\n").unwrap();

        let config = Config::new(root).await.unwrap();

        assert!(config.is_package_masked("app-editors/vim"));
        assert!(config.is_package_masked("sys-apps/util-linux"));
        assert!(!config.is_package_masked("app-misc/foo"));
    }

    #[tokio::test]
    async fn test_load_package_keywords_file() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_str().unwrap();

        // Create package.keywords file
        let package_keywords_dir = temp_dir.path().join("etc/portage/package.keywords");
        fs::create_dir_all(&package_keywords_dir).unwrap();
        fs::write(package_keywords_dir.join("test"), "app-editors/vim ~amd64\nsys-apps/util-linux amd64\n").unwrap();

        let config = Config::new(root).await.unwrap();

        let vim_keywords = config.get_package_keywords("app-editors/vim");
        assert_eq!(vim_keywords, Some(&vec!["~amd64".to_string()]));

        let util_keywords = config.get_package_keywords("sys-apps/util-linux");
        assert_eq!(util_keywords, Some(&vec!["amd64".to_string()]));
    }

    #[tokio::test]
    async fn test_load_sets_conf() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_str().unwrap();

        // Create sets.conf file
        let portage_dir = temp_dir.path().join("etc/portage");
        fs::create_dir_all(&portage_dir).unwrap();
        fs::write(portage_dir.join("sets.conf"), "my-editors app-editors/vim app-editors/emacs\nmy-tools sys-apps/util-linux\n").unwrap();

        let config = Config::new(root).await.unwrap();

        let editors_set = config.get_set_packages("my-editors");
        assert_eq!(editors_set, Some(&vec!["app-editors/vim".to_string(), "app-editors/emacs".to_string()]));

        let tools_set = config.get_set_packages("my-tools");
        assert_eq!(tools_set, Some(&vec!["sys-apps/util-linux".to_string()]));
    }

    #[tokio::test]
    async fn test_user_config_overrides_profile() {
        let temp_dir = TempDir::new().unwrap();
        let root = temp_dir.path().to_str().unwrap();

        // Create package.use file that overrides profile
        let package_use_dir = temp_dir.path().join("etc/portage/package.use");
        fs::create_dir_all(&package_use_dir).unwrap();
        fs::write(package_use_dir.join("test"), "app-editors/vim X gtk\n").unwrap();

        let config = Config::new(root).await.unwrap();

        // User config should take precedence
        let vim_flags = config.get_package_use_flags("app-editors/vim");
        assert_eq!(vim_flags, Some(&vec!["X".to_string(), "gtk".to_string()]));
    }
}