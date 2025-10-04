use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use crate::exception::InvalidData;

/// License manager for handling license acceptance
pub struct LicenseManager {
    root: String,
    accepted_licenses_file: PathBuf,
    package_license_dir: PathBuf,
}

impl LicenseManager {
    /// Create a new license manager
    pub fn new(root: &str) -> Self {
        let root_path = Path::new(root);
        Self {
            root: root.to_string(),
            accepted_licenses_file: root_path.join("var/lib/portage/license-accepted"),
            package_license_dir: root_path.join("etc/portage/package.license"),
        }
    }

    /// Parse a license string into individual license groups
    /// Handles || (or) syntax: "GPL-2 || ( LGPL-2.1 BSD )"
    /// Returns a vector of license groups, where each group is a vector of licenses.
    /// Any license in a group satisfies that group, and all groups must be satisfied.
    pub fn parse_license_string(license_str: &str) -> Result<Vec<Vec<String>>, InvalidData> {
        if license_str.trim().is_empty() {
            return Ok(vec![]);
        }

        // Split by || to get the main groups
        let or_groups: Vec<&str> = license_str.split("||").map(|s| s.trim()).collect();

        let mut result_groups = Vec::new();

        for group_str in or_groups {
            let group_str = group_str.trim();

            // Handle parenthesized groups
            if group_str.starts_with('(') && group_str.ends_with(')') {
                let inner = &group_str[1..group_str.len()-1];
                let licenses = Self::parse_license_group(inner)?;
                result_groups.push(licenses);
            } else {
                // Simple license or space-separated licenses
                let licenses = Self::parse_license_group(group_str)?;
                result_groups.push(licenses);
            }
        }

        Ok(result_groups)
    }

    /// Parse a single license group (space-separated licenses)
    fn parse_license_group(group_str: &str) -> Result<Vec<String>, InvalidData> {
        let mut licenses = Vec::new();

        for part in group_str.split_whitespace() {
            let part = part.trim();
            if !part.is_empty() {
                licenses.push(part.to_string());
            }
        }

        Ok(licenses)
    }

    /// Check if a license specification is accepted
    /// Returns true if any license group is fully accepted
    pub fn is_license_accepted(&self, license_str: &str) -> Result<bool, InvalidData> {
        let license_groups = Self::parse_license_string(license_str)?;

        if license_groups.is_empty() {
            // No license specified - assume accepted
            return Ok(true);
        }

        // Get accepted licenses
        let accepted_licenses = self.get_accepted_licenses()?;

        // Check if any group is fully accepted
        for group in &license_groups {
            if group.iter().all(|license| accepted_licenses.contains(license)) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    /// Get all accepted licenses from various sources
    pub fn get_accepted_licenses(&self) -> Result<HashSet<String>, InvalidData> {
        let mut accepted = HashSet::new();

        // Add licenses from /var/lib/portage/license-accepted
        if self.accepted_licenses_file.exists() {
            let content = fs::read_to_string(&self.accepted_licenses_file)
                .map_err(|e| InvalidData::new(&format!("Failed to read accepted licenses: {}", e), None))?;

            for line in content.lines() {
                let line = line.trim();
                if !line.is_empty() && !line.starts_with('#') {
                    accepted.insert(line.to_string());
                }
            }
        }

        // TODO: Add licenses from /etc/portage/package.license files
        // This would require parsing package.license files which can have complex syntax

        // Add common accepted licenses (Gentoo defaults)
        // In a real implementation, this would come from make.conf ACCEPT_LICENSE
        let default_accepted = [
            "GPL-2", "GPL-3", "LGPL-2.1", "LGPL-3", "BSD", "MIT", "Apache-2.0",
            "ISC", "CC0-1.0", "ZLIB", "Boost-1.0", "PostgreSQL", "OpenSSL",
        ];

        for license in &default_accepted {
            accepted.insert(license.to_string());
        }

        Ok(accepted)
    }

    /// Accept a license by adding it to the accepted licenses file
    pub fn accept_license(&self, license: &str) -> Result<(), InvalidData> {
        // Create directory if it doesn't exist
        if let Some(parent) = self.accepted_licenses_file.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| InvalidData::new(&format!("Failed to create license directory: {}", e), None))?;
        }

        // Read existing content
        let mut existing = if self.accepted_licenses_file.exists() {
            fs::read_to_string(&self.accepted_licenses_file)
                .map_err(|e| InvalidData::new(&format!("Failed to read accepted licenses: {}", e), None))?
        } else {
            String::new()
        };

        // Check if already accepted
        if existing.lines().any(|line| line.trim() == license) {
            return Ok(());
        }

        // Add the license
        if !existing.is_empty() && !existing.ends_with('\n') {
            existing.push('\n');
        }
        existing.push_str(license);
        existing.push('\n');

        fs::write(&self.accepted_licenses_file, existing)
            .map_err(|e| InvalidData::new(&format!("Failed to write accepted licenses: {}", e), None))?;

        Ok(())
    }

    /// Check licenses for a list of packages and prompt for acceptance if needed
    /// Returns true if all licenses are accepted or user accepts them
    pub async fn check_and_prompt_licenses(&self, packages: &[String], porttree: &mut crate::porttree::PortTree) -> Result<bool, InvalidData> {
        let mut unaccepted_licenses = Vec::new();

        // Collect all unique licenses that need acceptance
        for cpv in packages {
            if let Some(metadata) = porttree.get_metadata(cpv).await {
                if let Some(license_str) = metadata.get("LICENSE") {
                    if !self.is_license_accepted(license_str)? {
                        // Parse license groups and collect unaccepted ones
                        let groups = Self::parse_license_string(license_str)?;
                        for group in groups {
                            for license in group {
                                if !self.get_accepted_licenses()?.contains(&license) {
                                    unaccepted_licenses.push((cpv.clone(), license));
                                }
                            }
                        }
                    }
                }
            }
        }

        if unaccepted_licenses.is_empty() {
            return Ok(true);
        }

        // Display unaccepted licenses
        println!("The following packages have unaccepted licenses:");
        println!();

        let mut package_licenses: HashMap<String, Vec<String>> = HashMap::new();
        for (cpv, license) in &unaccepted_licenses {
            package_licenses.entry(cpv.clone()).or_insert_with(Vec::new).push(license.clone());
        }

        for (cpv, licenses) in &package_licenses {
            println!("{}: {}", cpv, licenses.join(", "));
        }

        println!();
        println!("Do you accept these licenses? [y/N]");

        // Read user input
        let mut input = String::new();
        match std::io::stdin().read_line(&mut input) {
            Ok(_) => {
                let response = input.trim().to_lowercase();
                if response == "y" || response == "yes" {
                    // Accept all the unaccepted licenses
                    for (_cpv, license) in &unaccepted_licenses {
                        self.accept_license(license)?;
                    }
                    println!("Licenses accepted.");
                    Ok(true)
                } else {
                    println!("Licenses not accepted. Aborting.");
                    Ok(false)
                }
            }
            Err(e) => {
                eprintln!("Failed to read user input: {}", e);
                Ok(false)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_parse_simple_license() {
        let result = LicenseManager::parse_license_string("GPL-2").unwrap();
        assert_eq!(result, vec![vec!["GPL-2"]]);
    }

    #[tokio::test]
    async fn test_parse_or_license() {
        let result = LicenseManager::parse_license_string("GPL-2 || BSD").unwrap();
        assert_eq!(result, vec![vec!["GPL-2"], vec!["BSD"]]);
    }

    #[tokio::test]
    async fn test_parse_parenthesized_or_license() {
        let result = LicenseManager::parse_license_string("GPL-2 || ( LGPL-2.1 BSD )").unwrap();
        assert_eq!(result, vec![vec!["GPL-2"], vec!["LGPL-2.1", "BSD"]]);
    }

    #[tokio::test]
    async fn test_parse_multiple_licenses_in_group() {
        let result = LicenseManager::parse_license_string("( GPL-2 LGPL-2.1 ) || BSD").unwrap();
        assert_eq!(result, vec![vec!["GPL-2", "LGPL-2.1"], vec!["BSD"]]);
    }

    #[tokio::test]
    async fn test_license_acceptance() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let manager = LicenseManager::new(temp_path);

        // Initially GPL-2 should be accepted (default)
        assert!(manager.is_license_accepted("GPL-2").unwrap());

        // Unknown license should not be accepted
        assert!(!manager.is_license_accepted("UNKNOWN-LICENSE").unwrap());

        // Accept the unknown license
        manager.accept_license("UNKNOWN-LICENSE").unwrap();

        // Now it should be accepted
        assert!(manager.is_license_accepted("UNKNOWN-LICENSE").unwrap());
    }

    #[tokio::test]
    async fn test_or_license_acceptance() {
        let temp_dir = tempfile::TempDir::new().unwrap();
        let temp_path = temp_dir.path().to_str().unwrap();

        let manager = LicenseManager::new(temp_path);

        // GPL-2 || BSD should be accepted since GPL-2 is in defaults
        assert!(manager.is_license_accepted("GPL-2 || BSD").unwrap());

        // GPL-2 || UNKNOWN should be accepted since GPL-2 is accepted
        assert!(manager.is_license_accepted("GPL-2 || UNKNOWN").unwrap());

        // UNKNOWN1 || UNKNOWN2 should not be accepted
        assert!(!manager.is_license_accepted("UNKNOWN1 || UNKNOWN2").unwrap());

        // Accept UNKNOWN1
        manager.accept_license("UNKNOWN1").unwrap();

        // Now UNKNOWN1 || UNKNOWN2 should be accepted
        assert!(manager.is_license_accepted("UNKNOWN1 || UNKNOWN2").unwrap());
    }
}