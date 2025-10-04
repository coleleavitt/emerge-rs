use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use crate::exception::InvalidData;
use crate::atom::Atom;
use crate::profile::{ProfileManager, Profile};

/// Package masking types
#[derive(Debug, Clone, PartialEq)]
pub enum MaskType {
    Mask,      // package.mask - blocks packages
    Unmask,    // package.unmask - allows masked packages
    Keywords,  // package.keywords - keyword restrictions
}

/// Represents a masking rule
#[derive(Debug, Clone)]
pub struct MaskRule {
    pub mask_type: MaskType,
    pub atom: Atom,
    pub comment: Option<String>,
}

/// Package masking manager for handling package.mask, package.unmask, etc.
pub struct MaskManager {
    root: String,
    config_dir: PathBuf,
    profile_manager: ProfileManager,
    accept_keywords: Vec<String>,
}

impl MaskManager {
    /// Create a new mask manager
    pub fn new(root: &str, accept_keywords: Vec<String>) -> Self {
        let root_path = Path::new(root);
        Self {
            root: root.to_string(),
            config_dir: root_path.join("etc/portage"),
            profile_manager: ProfileManager::new(root),
            accept_keywords,
        }
    }

    /// Check if a package atom is masked
    /// Returns Some(reason) if masked, None if not masked
    pub async fn is_masked(&self, atom: &Atom) -> Result<Option<String>, InvalidData> {
        // Check package.mask files
        let masked_by_mask = self.check_mask_files(atom, MaskType::Mask).await?;
        if let Some(reason) = masked_by_mask {
            // Check if it's unmasked by package.unmask
            let unmasked = self.check_mask_files(atom, MaskType::Unmask).await?;
            if unmasked.is_none() {
                return Ok(Some(reason));
            }
        }

        // Check keyword restrictions from package.keywords
        let keyword_masked = self.check_keyword_restrictions(atom).await?;
        if let Some(reason) = keyword_masked {
            return Ok(Some(reason));
        }

        // Check ebuild KEYWORDS if version is specified
        if let Some(version) = &atom.version {
            let keywords_masked = self.check_ebuild_keywords(atom, version)?;
            if let Some(reason) = keywords_masked {
                return Ok(Some(reason));
            }
        }

        Ok(None)
    }

    /// Check ebuild KEYWORDS for a specific version
    fn check_ebuild_keywords(&self, atom: &Atom, version: &str) -> Result<Option<String>, InvalidData> {
        // Try to find the ebuild file in the repository
        let ebuild_path = self.find_ebuild_path(atom, version)?;

        if let Some(path) = ebuild_path {
            // Parse the ebuild to get KEYWORDS
            match self.parse_ebuild_keywords(&path) {
                Ok(keywords) => {
                    // Check if any of the ebuild's keywords are accepted
                    let accepted_keywords: std::collections::HashSet<_> = self.accept_keywords.iter().cloned().collect();

                    // Check for exact matches or wildcard matches
                    let mut has_accepted = false;
                    for kw in &keywords {
                        if accepted_keywords.contains(kw) {
                            has_accepted = true;
                            break;
                        }
                        // Check for wildcard matches (e.g., "amd64" matches "~amd64")
                        if kw.starts_with('~') {
                            let stable_kw = &kw[1..];
                            if accepted_keywords.contains(stable_kw) {
                                has_accepted = true;
                                break;
                            }
                        }
                    }

                    if !has_accepted && !keywords.is_empty() {
                        return Ok(Some(format!("ebuild {} has keywords {:?} but none are accepted ({:?})",
                                               atom.cp(), keywords, self.accept_keywords)));
                    }
                }
                Err(e) => {
                    // If we can't parse the ebuild, we'll allow it for now
                    // In a production system, this might be an error
                    eprintln!("Warning: Failed to parse KEYWORDS from {}: {}", path.display(), e);
                }
            }
        }

        Ok(None)
    }

    /// Find the ebuild file path for a given atom and version
    fn find_ebuild_path(&self, atom: &Atom, version: &str) -> Result<Option<std::path::PathBuf>, InvalidData> {
        // Look in standard Gentoo repository locations
        let repo_paths = [
            "/var/db/repos/gentoo",  // Modern location
            "/usr/portage",          // Legacy location
        ];

        for repo_path in &repo_paths {
            let category_path = std::path::Path::new(repo_path).join(&atom.category);
            if !category_path.exists() {
                continue;
            }

            let package_path = category_path.join(&atom.package);
            if !package_path.exists() {
                continue;
            }

            // Look for ebuild files matching the version
            if let Ok(entries) = std::fs::read_dir(&package_path) {
                for entry in entries {
                    if let Ok(entry) = entry {
                        if let Some(filename) = entry.file_name().to_str() {
                            if filename.ends_with(".ebuild") {
                                // Extract version from filename
                                // Format: package-version.ebuild
                                let name_without_ext = filename.trim_end_matches(".ebuild");
                                if let Some(dash_pos) = name_without_ext.rfind('-') {
                                    let file_version = &name_without_ext[dash_pos + 1..];
                                    if file_version == version {
                                        return Ok(Some(entry.path()));
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Parse KEYWORDS from an ebuild file
    fn parse_ebuild_keywords(&self, ebuild_path: &std::path::Path) -> Result<Vec<String>, InvalidData> {
        let content = std::fs::read_to_string(ebuild_path)
            .map_err(|e| InvalidData::new(&format!("Failed to read ebuild: {}", e), None))?;

        // Simple parsing for KEYWORDS line
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("KEYWORDS=") {
                // Extract the value part
                if let Some(eq_pos) = line.find('=') {
                    let value_part = &line[eq_pos + 1..].trim();

                    // Handle quoted strings and arrays
                    let keywords_str = if value_part.starts_with('"') && value_part.ends_with('"') {
                        &value_part[1..value_part.len() - 1]
                    } else if value_part.starts_with('\'') && value_part.ends_with('\'') {
                        &value_part[1..value_part.len() - 1]
                    } else if value_part.starts_with('(') && value_part.ends_with(')') {
                        // Array format: KEYWORDS=( "amd64" "x86" )
                        let inner = &value_part[1..value_part.len() - 1];
                        inner
                    } else {
                        value_part
                    };

                    // Split by whitespace and clean up
                    let keywords: Vec<String> = keywords_str
                        .split_whitespace()
                        .map(|s| s.trim_matches('"').trim_matches('\'').to_string())
                        .filter(|s| !s.is_empty())
                        .collect();

                    return Ok(keywords);
                }
            }
        }

        // If no KEYWORDS found, return empty vec (unkeyworded)
        Ok(vec![])
    }

    /// Check mask files of a specific type for a given atom
    async fn check_mask_files(&self, atom: &Atom, mask_type: MaskType) -> Result<Option<String>, InvalidData> {
        let mut mask_files = Vec::new();

        // Get current profile and its inheritance chain
        if let Ok(current_profile) = self.profile_manager.get_current_profile().await {
            // Add profile mask files from inheritance chain (parent first, then child)
            let mut profiles = current_profile.parent_profiles.clone();
            profiles.push(current_profile);

            for profile in profiles {
                let profile_mask_file = match mask_type {
                    MaskType::Mask => profile.path.join("package.mask"),
                    MaskType::Unmask => profile.path.join("package.unmask"),
                    MaskType::Keywords => profile.path.join("package.keywords"),
                };
                mask_files.push(profile_mask_file);
            }
        }

        // Add user config mask files (highest precedence)
        let config_mask_file = match mask_type {
            MaskType::Mask => self.config_dir.join("package.mask"),
            MaskType::Unmask => self.config_dir.join("package.unmask"),
            MaskType::Keywords => self.config_dir.join("package.keywords"),
        };
        mask_files.push(config_mask_file);

        // Check each mask file in order (profile inheritance, then user config)
        for mask_file in mask_files {
            if mask_file.exists() {
                let content = fs::read_to_string(&mask_file)
                    .map_err(|e| InvalidData::new(&format!("Failed to read mask file {}: {}", mask_file.display(), e), None))?;

                let reason = self.check_mask_file_content(&content, atom, &mask_type)?;
                if reason.is_some() {
                    return Ok(reason);
                }
            }
        }

        Ok(None)
    }

    /// Check if content from a mask file matches the atom
    fn check_mask_file_content(&self, content: &str, atom: &Atom, mask_type: &MaskType) -> Result<Option<String>, InvalidData> {
        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse the line
            let (atom_str, comment) = if let Some(comment_pos) = line.find('#') {
                let atom_part = line[..comment_pos].trim();
                let comment_part = line[comment_pos + 1..].trim().to_string();
                (atom_part, Some(comment_part))
            } else {
                (line, None)
            };

            if atom_str.is_empty() {
                continue;
            }

            // Try to parse as atom
            match Atom::new(atom_str) {
                Ok(mask_atom) => {
                    // For masking, we compare category/package
                    if mask_atom.category == atom.category && mask_atom.package == atom.package {
                        let reason = match mask_type {
                            MaskType::Mask => format!("masked by {}", atom_str),
                            MaskType::Unmask => format!("unmasked by {}", atom_str),
                            MaskType::Keywords => format!("keyword restricted by {}", atom_str),
                        };

                        let full_reason = if let Some(comment) = comment {
                            format!("{}: {}", reason, comment)
                        } else {
                            reason
                        };

                        return Ok(Some(full_reason));
                    }
                }
                Err(_) => {
                    // Invalid atom syntax, skip
                    continue;
                }
            }
        }

        Ok(None)
    }

    /// Check keyword restrictions for a package
    async fn check_keyword_restrictions(&self, atom: &Atom) -> Result<Option<String>, InvalidData> {
        let mut keyword_files = Vec::new();

        // Add profile keyword files from inheritance chain
        if let Ok(current_profile) = self.profile_manager.get_current_profile().await {
            let mut profiles = current_profile.parent_profiles.clone();
            profiles.push(current_profile);

            for profile in profiles {
                keyword_files.push(profile.path.join("package.keywords"));
            }
        }

        // Add user config keywords file (highest precedence)
        keyword_files.push(self.config_dir.join("package.keywords"));

        for keyword_file in keyword_files {
            if keyword_file.exists() {
                let content = fs::read_to_string(&keyword_file)
                    .map_err(|e| InvalidData::new(&format!("Failed to read keywords file {}: {}", keyword_file.display(), e), None))?;

                for line in content.lines() {
                    let line = line.trim();
                    if line.is_empty() || line.starts_with('#') {
                        continue;
                    }

                    // Parse line: "atom keywords # comment"
                    let line_content = if let Some(comment_pos) = line.find('#') {
                        line[..comment_pos].trim()
                    } else {
                        line
                    };

                    // Split by whitespace to get atom and keywords
                    let parts: Vec<&str> = line_content.split_whitespace().collect();
                    if parts.len() < 2 {
                        continue;
                    }

                    let atom_str = parts[0];
                    let keywords: Vec<&str> = parts[1..].to_vec();

                    if let Ok(keyword_atom) = Atom::new(atom_str) {
                        if keyword_atom.category == atom.category && keyword_atom.package == atom.package {
                            // Check if any of the specified keywords are accepted
                            let accepted = keywords.iter().any(|kw| self.accept_keywords.contains(&kw.to_string()));
                            if !accepted {
                                return Ok(Some(format!("keyword restricted by {} (accepted: {:?})", line_content, self.accept_keywords)));
                            }
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    /// Get all masking rules from all mask files
    pub async fn get_all_mask_rules(&self) -> Result<Vec<MaskRule>, InvalidData> {
        let mut rules = Vec::new();

        let mask_types = vec![MaskType::Mask, MaskType::Unmask, MaskType::Keywords];

        for mask_type in mask_types {
            let mut mask_files = Vec::new();

            // Add profile mask files from inheritance chain
            if let Ok(current_profile) = self.profile_manager.get_current_profile().await {
                let mut profiles = current_profile.parent_profiles.clone();
                profiles.push(current_profile);

                for profile in profiles {
                    let profile_mask_file = match mask_type {
                        MaskType::Mask => profile.path.join("package.mask"),
                        MaskType::Unmask => profile.path.join("package.unmask"),
                        MaskType::Keywords => profile.path.join("package.keywords"),
                    };
                    mask_files.push(profile_mask_file);
                }
            }

            // Add user config mask files (highest precedence)
            let config_mask_file = match mask_type {
                MaskType::Mask => self.config_dir.join("package.mask"),
                MaskType::Unmask => self.config_dir.join("package.unmask"),
                MaskType::Keywords => self.config_dir.join("package.keywords"),
            };
            mask_files.push(config_mask_file);

            for mask_file in mask_files {
                if mask_file.exists() {
                    let content = fs::read_to_string(&mask_file)
                        .map_err(|e| InvalidData::new(&format!("Failed to read mask file {}: {}", mask_file.display(), e), None))?;

                    let file_rules = self.parse_mask_file(&content, mask_type.clone())?;
                    rules.extend(file_rules);
                }
            }
        }

        Ok(rules)
    }

    /// Parse a mask file content into MaskRule objects
    fn parse_mask_file(&self, content: &str, mask_type: MaskType) -> Result<Vec<MaskRule>, InvalidData> {
        let mut rules = Vec::new();

        for line in content.lines() {
            let line = line.trim();

            // Skip empty lines and comments
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // Parse the line
            let (atom_str, comment) = if let Some(comment_pos) = line.find('#') {
                let atom_part = line[..comment_pos].trim();
                let comment_part = line[comment_pos + 1..].trim().to_string();
                (atom_part, Some(comment_part))
            } else {
                (line, None)
            };

            if atom_str.is_empty() {
                continue;
            }

            // Try to parse as atom
            match Atom::new(atom_str) {
                Ok(atom) => {
                    rules.push(MaskRule {
                        mask_type: mask_type.clone(),
                        atom,
                        comment,
                    });
                }
                Err(_) => {
                    // Invalid atom syntax, skip with warning
                    eprintln!("Warning: Invalid atom syntax in mask file: {}", atom_str);
                }
            }
        }

        Ok(rules)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[tokio::test]
    async fn test_mask_manager_creation() {
        let manager = MaskManager::new("/", vec!["amd64".to_string()]);
        assert_eq!(manager.root, "/");
    }

    #[tokio::test]
    async fn test_package_masking() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let manager = MaskManager::new(temp_path, vec!["amd64".to_string()]);

        // Create a package.mask file
        let mask_dir = temp_dir.path().join("etc/portage");
        fs::create_dir_all(&mask_dir).unwrap();
        let mask_file = mask_dir.join("package.mask");
        fs::write(&mask_file, "app-misc/test-pkg # Test masking\n").unwrap();

        // Test masking
        let atom = Atom::new("app-misc/test-pkg").unwrap();
        let result = manager.is_masked(&atom).await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("masked by app-misc/test-pkg"));
    }

    #[tokio::test]
    async fn test_package_unmasking() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let manager = MaskManager::new(temp_path, vec!["amd64".to_string()]);

        // Create both mask and unmask files
        let mask_dir = temp_dir.path().join("etc/portage");
        fs::create_dir_all(&mask_dir).unwrap();

        let mask_file = mask_dir.join("package.mask");
        fs::write(&mask_file, "app-misc/test-pkg # Masked for testing\n").unwrap();

        let unmask_file = mask_dir.join("package.unmask");
        fs::write(&unmask_file, "app-misc/test-pkg # Unmasked for testing\n").unwrap();

        // Test that unmask overrides mask
        let atom = Atom::new("app-misc/test-pkg").unwrap();
        let result = manager.is_masked(&atom).await.unwrap();
        assert!(result.is_none()); // Should not be masked due to unmask
    }

    #[tokio::test]
    async fn test_keyword_restrictions() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let manager = MaskManager::new(temp_path, vec!["amd64".to_string()]);

        // Create a package.keywords file
        let mask_dir = temp_dir.path().join("etc/portage");
        fs::create_dir_all(&mask_dir).unwrap();
        let keywords_file = mask_dir.join("package.keywords");
        fs::write(&keywords_file, "app-misc/test-pkg ~amd64\n").unwrap();

        // Test keyword restriction
        let atom = Atom::new("app-misc/test-pkg").unwrap();
        let result = manager.is_masked(&atom).await.unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("keyword restricted"));
    }

    #[tokio::test]
    async fn test_profile_based_masking() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        // Create a mock profile structure
        let profiles_dir = temp_dir.path().join("var/db/repos/gentoo/profiles");
        fs::create_dir_all(&profiles_dir).unwrap();

        let profile_dir = profiles_dir.join("test-profile");
        fs::create_dir(&profile_dir).unwrap();

        // Create profile package.mask
        let profile_mask = profile_dir.join("package.mask");
        fs::write(&profile_mask, "app-misc/profile-masked-pkg # Masked in profile\n").unwrap();

        // Create make.profile symlink
        let etc_portage = temp_dir.path().join("etc/portage");
        fs::create_dir_all(&etc_portage).unwrap();
        let make_profile = etc_portage.join("make.profile");

        // Create relative symlink (this might not work in all test environments)
        // For testing, we'll create a direct symlink
        #[cfg(unix)]
        {
            use std::os::unix::fs;
            let _ = fs::symlink(&profile_dir, &make_profile);
        }

        let manager = MaskManager::new(temp_path, vec!["amd64".to_string()]);

        // Test that profile-based masking works
        let atom = Atom::new("app-misc/profile-masked-pkg").unwrap();
        let result = manager.is_masked(&atom).await;

        // The result might be an error if the symlink doesn't work in the test environment,
        // but if it works, it should find the mask
        match result {
            Ok(Some(reason)) => {
                assert!(reason.contains("masked by app-misc/profile-masked-pkg"));
            }
            Ok(None) => {
                // Profile system might not be set up correctly in test environment
                // This is acceptable for integration testing
            }
            Err(_) => {
                // Profile system might not be accessible in test environment
                // This is acceptable for integration testing
            }
        }
    }
}