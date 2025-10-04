// porttree.rs -- Portage tree API for repository scanning

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::fs as tokio_fs;
use tokio::process::Command;

#[derive(Debug)]
pub struct PortTree {
    pub root: String,
    pub repositories: HashMap<String, Repository>,
    pub main_repo: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SyncMetadata {
    pub last_sync: Option<u64>,     // Unix timestamp of last successful sync
    pub last_attempt: Option<u64>, // Unix timestamp of last sync attempt
    pub success: bool,             // Whether last sync was successful
    pub error_message: Option<String>, // Error message from last failed sync
}

#[derive(Debug, Clone)]
pub struct Repository {
    pub name: String,
    pub location: String,
    pub sync_type: Option<String>, // rsync, git, etc.
    pub sync_uri: Option<String>,  // URI to sync from
    pub auto_sync: bool,           // whether to sync automatically
    pub sync_depth: Option<i32>,   // git sync depth
    pub sync_hooks_only_on_change: bool, // optimization flag
    pub sync_metadata: SyncMetadata,
    pub eclass_cache: HashMap<String, String>,
    pub metadata_cache: HashMap<String, HashMap<String, String>>,
}

impl PortTree {
    pub fn new(root: &str) -> Self {
        PortTree {
            root: root.to_string(),
            repositories: HashMap::new(),
            main_repo: None,
        }
    }

    pub fn scan_repositories(&mut self) {
        let repos_conf_paths = [
            "/etc/portage/repos.conf",
            "/usr/share/portage/config/repos.conf",
        ];

        for conf_path in &repos_conf_paths {
            let path = Path::new(conf_path);
            
            if path.is_dir() {
                if let Ok(entries) = fs::read_dir(path) {
                    for entry in entries.flatten() {
                        let entry_path = entry.path();
                        if entry_path.is_file() && entry_path.extension().and_then(|s| s.to_str()) == Some("conf") {
                            if let Ok(content) = fs::read_to_string(&entry_path) {
                                self.parse_repos_conf(&content);
                            }
                        }
                    }
                }
            } else if path.is_file() {
                if let Ok(content) = fs::read_to_string(conf_path) {
                    self.parse_repos_conf(&content);
                }
            }
        }

        // If no repositories found, add default gentoo repo
        if self.repositories.is_empty() {
            let repo = Repository {
                name: "gentoo".to_string(),
                location: "/usr/portage".to_string(),
                sync_type: Some("rsync".to_string()),
                sync_uri: Some("rsync://rsync.gentoo.org/gentoo-portage".to_string()),
                auto_sync: true,
                sync_depth: None,
                sync_hooks_only_on_change: false,
                sync_metadata: SyncMetadata {
                    last_sync: None,
                    last_attempt: None,
                    success: false,
                    error_message: None,
                },
                eclass_cache: HashMap::new(),
                metadata_cache: HashMap::new(),
            };
            self.repositories.insert("gentoo".to_string(), repo);
        }
    }

    pub fn parse_repos_conf(&mut self, content: &str) {
        let mut current_section = String::new();
        let mut current_repo: Option<Repository> = None;
        let mut in_default_section = false;

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                if let Some(repo) = current_repo.take() {
                    if !repo.location.is_empty() {
                        self.repositories.insert(repo.name.clone(), repo);
                    }
                }

                current_section = line[1..line.len()-1].to_string();
                
                if current_section.to_uppercase() == "DEFAULT" {
                    in_default_section = true;
                    current_repo = None;
                    continue;
                }
                
                in_default_section = false;
                current_repo = Some(Repository {
                    name: current_section.clone(),
                    location: String::new(),
                    sync_type: None,
                    sync_uri: None,
                    auto_sync: true,
                    sync_depth: None,
                    sync_hooks_only_on_change: false,
                    sync_metadata: SyncMetadata {
                        last_sync: None,
                        last_attempt: None,
                        success: false,
                        error_message: None,
                    },
                    eclass_cache: HashMap::new(),
                    metadata_cache: HashMap::new(),
                });
            } else if in_default_section {
                if let Some(eq_pos) = line.find('=') {
                    let key = line[..eq_pos].trim();
                    let value = line[eq_pos + 1..].trim().trim_matches('"');
                    
                    if key == "main-repo" {
                        self.main_repo = Some(value.to_string());
                    }
                }
            } else if let Some(ref mut repo) = current_repo {
                if let Some(eq_pos) = line.find('=') {
                    let key = line[..eq_pos].trim();
                    let value = line[eq_pos + 1..].trim().trim_matches('"');

                    match key {
                        "location" => repo.location = value.to_string(),
                        "sync-type" => repo.sync_type = Some(value.to_string()),
                        "sync-uri" => repo.sync_uri = Some(value.to_string()),
                        "auto-sync" => repo.auto_sync = value.to_lowercase() == "true" || value == "yes",
                        "sync-depth" => {
                            if let Ok(depth) = value.parse::<i32>() {
                                repo.sync_depth = Some(depth);
                            }
                        }
                        "sync-hooks-only-on-change" => {
                            repo.sync_hooks_only_on_change = value.to_lowercase() == "true" || value == "yes";
                        }
                        _ => {} // Ignore unknown keys
                    }
                }
            }
        }

        if let Some(repo) = current_repo {
            if !repo.location.is_empty() {
                self.repositories.insert(repo.name.clone(), repo);
            }
        }
    }

    /// Validate that a repository exists and has basic structure
    pub fn validate_repository(&self, repo_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let repo = self.repositories.get(repo_name)
            .ok_or_else(|| format!("Repository {} not found", repo_name))?;

        let repo_path = Path::new(&repo.location);

        // Check if repository directory exists
        if !repo_path.exists() {
            return Err(format!("Repository path does not exist: {}", repo.location).into());
        }

        // Check for basic repository structure (at least a profiles directory)
        let profiles_dir = repo_path.join("profiles");
        if !profiles_dir.exists() || !profiles_dir.is_dir() {
            return Err(format!("Repository {} missing profiles directory", repo_name).into());
        }

        // Check for eclass directory
        let eclass_dir = repo_path.join("eclass");
        if !eclass_dir.exists() || !eclass_dir.is_dir() {
            return Err(format!("Repository {} missing eclass directory", repo_name).into());
        }

        Ok(())
    }

    pub fn get_ebuild_path(&self, cpv: &str) -> Option<String> {
        // Parse CPV: category/package-version
        let parts: Vec<&str> = cpv.split('/').collect();
        if parts.len() != 2 {
            return None;
        }

        let category = parts[0];
        let pkg_version = parts[1];

        // Find the last dash to separate package name from version
        if let Some(last_dash) = pkg_version.rfind('-') {
            let package = &pkg_version[..last_dash];
            let version = &pkg_version[last_dash + 1..];

            // Check each repository
            for repo in self.repositories.values() {
                let ebuild_path = format!("{}/{}/{}/{}-{}.ebuild",
                    repo.location, category, package, package, version);

                if std::path::Path::new(&ebuild_path).exists() {
                    return Some(ebuild_path);
                }
            }
        }

        None
    }

    pub async fn get_metadata(&mut self, cpv: &str) -> Option<HashMap<String, String>> {
        // Check cache first
        for repo in self.repositories.values() {
            if let Some(cached) = repo.metadata_cache.get(cpv) {
                return Some(cached.clone());
            }
        }

        // Not in cache, try to load from ebuild
        if let Some(ebuild_path) = self.get_ebuild_path(cpv) {
            if let Ok(content) = tokio::fs::read_to_string(&ebuild_path).await {
                use crate::doebuild::Ebuild;
                if let Ok(metadata) = Ebuild::parse_metadata_with_use(&content, &std::collections::HashMap::new(), "", "", "") {
                    let mut meta = HashMap::new();
                    meta.insert("DESCRIPTION".to_string(), metadata.description.unwrap_or_default());
                    meta.insert("HOMEPAGE".to_string(), metadata.homepage.unwrap_or_default());
                    meta.insert("LICENSE".to_string(), metadata.license.unwrap_or_default());
                    meta.insert("SLOT".to_string(), metadata.slot);
                    meta.insert("KEYWORDS".to_string(), metadata.keywords.join(" "));
                    meta.insert("IUSE".to_string(), metadata.iuse.join(" "));
                    meta.insert("DEPEND".to_string(), metadata.depend.iter().map(|a| a.cpv.clone()).collect::<Vec<_>>().join(" "));
                    meta.insert("RDEPEND".to_string(), metadata.rdepend.iter().map(|a| a.cpv.clone()).collect::<Vec<_>>().join(" "));
                    meta.insert("PDEPEND".to_string(), metadata.pdepend.iter().map(|a| a.cpv.clone()).collect::<Vec<_>>().join(" "));

                    // Cache the metadata in the appropriate repository
                    self.cache_metadata(cpv, meta.clone());
                    return Some(meta);
                }
            }
        }

        None
    }

    /// Cache metadata for a package
    pub fn cache_metadata(&mut self, cpv: &str, metadata: HashMap<String, String>) {
        // Find the repository that contains this package
        if let Some(ebuild_path) = self.get_ebuild_path(cpv) {
            for repo in self.repositories.values_mut() {
                if ebuild_path.starts_with(&repo.location) {
                    repo.metadata_cache.insert(cpv.to_string(), metadata);
                    return;
                }
            }
        }

        // Fallback: cache in first repository if no specific repo found
        if let Some(repo) = self.repositories.values_mut().next() {
            repo.metadata_cache.insert(cpv.to_string(), metadata);
        }
    }

    /// Check if a package exists (has any ebuilds) in the repositories
    pub fn package_exists(&self, cp: &str) -> bool {
        for repo in self.repositories.values() {
            let repo_path = Path::new(&repo.location);
            let pkg_path = repo_path.join(cp);
            if pkg_path.exists() && pkg_path.is_dir() {
                // Check if there are any .ebuild files
                if let Ok(entries) = std::fs::read_dir(&pkg_path) {
                    for entry in entries {
                        if let Ok(entry) = entry {
                            if let Some(file_name) = entry.path().file_name().and_then(|n| n.to_str()) {
                                if file_name.ends_with(".ebuild") {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Clear metadata cache
    pub fn clear_metadata_cache(&mut self) {
        for repo in self.repositories.values_mut() {
            repo.metadata_cache.clear();
        }
    }

    /// Pre-cache metadata for all packages in a repository
    pub async fn cache_all_metadata(&mut self, repo_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let repo = self.repositories.get(repo_name)
            .ok_or_else(|| format!("Repository {} not found", repo_name))?;

        let repo_path = Path::new(&repo.location);

        // Walk through all category directories
        if let Ok(entries) = std::fs::read_dir(repo_path) {
            for entry in entries {
                if let Ok(entry) = entry {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_dir() {
                            let category_path = entry.path();
                            if let Some(category_name) = category_path.file_name().and_then(|n| n.to_str()) {
                                // Skip non-category directories (like .git, metadata, etc.)
                                if category_name.starts_with('.') || category_name == "metadata" {
                                    continue;
                                }

                                // Walk through package directories in this category
                                if let Ok(pkg_entries) = fs::read_dir(&category_path) {
                                    for pkg_entry in pkg_entries {
                                        if let Ok(pkg_entry) = pkg_entry {
                                            if let Ok(pkg_file_type) = pkg_entry.file_type() {
                                                if pkg_file_type.is_dir() {
                                                    let pkg_path = pkg_entry.path();
                                                    if let Some(pkg_name) = pkg_path.file_name().and_then(|n| n.to_str()) {
                                                        // Walk through ebuild files in this package
                                                        if let Ok(ebuild_entries) = fs::read_dir(&pkg_path) {
                                                            for ebuild_entry in ebuild_entries {
                                                                if let Ok(ebuild_entry) = ebuild_entry {
                                                                    if let Some(file_name) = ebuild_entry.path().file_name().and_then(|n| n.to_str()) {
                                                                        if file_name.ends_with(".ebuild") {
                                                                            // Extract version from filename (package-version.ebuild)
                                                                            if let Some(version_start) = file_name.find('-') {
                                                                                if let Some(version_end) = file_name.rfind('.') {
                                                                                    let version = &file_name[version_start + 1..version_end];
                                                                                    let cpv = format!("{}/{}-{}", category_name, pkg_name, version);

                                                                                    // Cache metadata for this CPV
                                                                                     if let Some(_) = self.get_metadata(&cpv).await {
                                                                                        // get_metadata will cache it automatically
                                                                                    }
                                                                                }
                                                                            }
                                                                        }
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Validate repository integrity
    pub async fn validate_repository_integrity(&self, repo_name: &str) -> Result<(), Box<dyn std::error::Error>> {
        let repo = self.repositories.get(repo_name)
            .ok_or_else(|| format!("Repository {} not found", repo_name))?;

        let repo_path = Path::new(&repo.location);

        // Check if repository exists
        if !repo_path.exists() {
            return Err(format!("Repository path does not exist: {}", repo.location).into());
        }

        // For git repos, check if HEAD exists and is valid
        let git_dir = repo_path.join(".git");
        if git_dir.exists() {
            let head_file = git_dir.join("HEAD");
            if !head_file.exists() {
                return Err("Git repository missing HEAD file".into());
            }

            // Check if we can get current commit
            let output = Command::new("git")
                .args(&["rev-parse", "HEAD"])
                .current_dir(repo_path)
                .output()
                .await?;

            if !output.status.success() {
                return Err("Git repository HEAD is invalid".into());
            }
        }

        let is_main_repo = self.main_repo.as_ref().map(|m| m == repo_name).unwrap_or(false);
        
        if is_main_repo {
            let core_dirs = ["app-admin", "app-misc", "sys-apps", "dev-lang"];
            let mut missing_dirs = Vec::new();
            
            for dir in &core_dirs {
                if !repo_path.join(dir).exists() {
                    missing_dirs.push(*dir);
                }
            }
            
            if !missing_dirs.is_empty() {
                eprintln!("Warning: Main repository {} missing core directories: {}", 
                    repo_name, missing_dirs.join(", "));
            }
        }

        Ok(())
    }

    /// Load sync metadata from disk
    pub async fn load_sync_metadata(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        for repo in self.repositories.values_mut() {
            let metadata_file = Path::new(&repo.location).join(".sync_metadata");
            if metadata_file.exists() {
                let content = tokio_fs::read_to_string(&metadata_file).await?;
                if let Ok(metadata) = serde_json::from_str::<SyncMetadata>(&content) {
                    repo.sync_metadata = metadata;
                }
            }
        }
        Ok(())
    }

    /// Save sync metadata to disk
    pub async fn save_sync_metadata(&self) -> Result<(), Box<dyn std::error::Error>> {
        for repo in self.repositories.values() {
            let metadata_file = Path::new(&repo.location).join(".sync_metadata");
            let content = serde_json::to_string_pretty(&repo.sync_metadata)?;
            tokio_fs::write(&metadata_file, content).await?;
        }
        Ok(())
    }

    /// Update sync metadata after a sync attempt
    pub fn update_sync_metadata(&mut self, repo_name: &str, success: bool, error_message: Option<String>) {
        if let Some(repo) = self.repositories.get_mut(repo_name) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            repo.sync_metadata.last_attempt = Some(now);
            repo.sync_metadata.success = success;
            repo.sync_metadata.error_message = error_message;

            if success {
                repo.sync_metadata.last_sync = Some(now);
            }
        }
    }

    /// Get sync status for a repository
    pub fn get_sync_status(&self, repo_name: &str) -> Option<&SyncMetadata> {
        self.repositories.get(repo_name).map(|r| &r.sync_metadata)
    }

    /// Check if a repository needs syncing (based on auto-sync and time since last sync)
    pub fn needs_sync(&self, repo_name: &str) -> bool {
        if let Some(repo) = self.repositories.get(repo_name) {
            if !repo.auto_sync {
                return false;
            }

            // If never synced, needs sync
            if repo.sync_metadata.last_sync.is_none() {
                return true;
            }

            // Check if it's been more than 24 hours since last sync
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            if let Some(last_sync) = repo.sync_metadata.last_sync {
                now.saturating_sub(last_sync) > 86400 // 24 hours in seconds
            } else {
                true
            }
        } else {
            false
        }
    }
}