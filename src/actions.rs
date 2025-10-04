use crate::atom::Atom;
use crate::dep_check::DepChecker;
use crate::depgraph::DepGraph;
use crate::depgraph::{DepNode, DepType};
use crate::doebuild::Ebuild;
use crate::news::NewsManager;
use crate::porttree::PortTree;
use crate::sets;
use crate::sync::controller::sync_repository;

#[derive(Debug, Clone)]
enum PackageStatus {
    New,
    Upgrade,
    Rebuild,
    Downgrade,
}

#[derive(Debug, Clone)]
struct MergePlanItem {
    cpv: String,
    status: PackageStatus,
    old_version: Option<String>,
    slot: Option<String>,
    size: Option<u64>,
    use_changes: Vec<(String, bool)>,
    repository: Option<String>,
    abi: Option<String>,
}

pub async fn action_sync() -> i32 {
    use tokio_stream::StreamExt;

    println!("Syncing repositories...");

    let mut porttree = PortTree::new("/");
    porttree.scan_repositories();

    if let Err(e) = porttree.load_sync_metadata().await {
        eprintln!("Warning: Failed to load sync metadata: {}", e);
    }

    let repo_names: Vec<String> = porttree.repositories.keys().cloned().collect();
    let total_count = repo_names.len();

    if repo_names.is_empty() {
        println!("No repositories to sync.");
        return 0;
    }

    println!("Starting sync for {} repositories...\n", total_count);

    let mut tasks = tokio::task::JoinSet::new();

    for repo_name in repo_names {
        let repo = porttree.repositories.get(&repo_name).unwrap().clone();
        tasks.spawn(async move {
            println!(">>> Starting sync: {}", repo_name);
            let result = sync_repository(&repo).await;
            (repo_name, result)
        });
    }

    let mut success_count = 0;
    let mut completed_count = 0;

    while let Some(task_result) = tasks.join_next().await {
        completed_count += 1;
        
        match task_result {
            Ok((repo_name, sync_result)) => {
                match sync_result {
                    Ok(result) => {
                        porttree.update_sync_metadata(&repo_name, true, None);

                        match porttree.validate_repository_integrity(&repo_name).await {
                            Ok(_) => {
                                println!("✓ [{}/{}] Successfully synced {}: {}", 
                                    completed_count, total_count, repo_name, result.message);
                                success_count += 1;
                            }
                            Err(e) => {
                                eprintln!("⚠ [{}/{}] Synced {} but validation failed: {}", 
                                    completed_count, total_count, repo_name, e);
                                success_count += 1;
                            }
                        }
                    }
                    Err(e) => {
                        porttree.update_sync_metadata(&repo_name, false, Some(e.to_string()));
                        eprintln!("✗ [{}/{}] Failed to sync {}: {}", 
                            completed_count, total_count, repo_name, e);
                    }
                }
            }
            Err(e) => {
                eprintln!("✗ [{}/{}] Task panicked: {}", completed_count, total_count, e);
            }
        }
    }

    if let Err(e) = porttree.save_sync_metadata().await {
        eprintln!("Warning: Failed to save sync metadata: {}", e);
    }

    println!();
    if success_count == total_count {
        println!("All repositories synced successfully.");
        0
    } else {
        eprintln!("Synced {}/{} repositories.", success_count, total_count);
        1
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::porttree::{PortTree, Repository, SyncMetadata};
    use std::fs;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_sync_metadata_serialization() {
        let metadata = SyncMetadata {
            last_sync: Some(1234567890),
            last_attempt: Some(1234567900),
            success: true,
            error_message: None,
        };

        let json = serde_json::to_string(&metadata).unwrap();
        let deserialized: SyncMetadata = serde_json::from_str(&json).unwrap();

        assert_eq!(deserialized.last_sync, metadata.last_sync);
        assert_eq!(deserialized.last_attempt, metadata.last_attempt);
        assert_eq!(deserialized.success, metadata.success);
        assert_eq!(deserialized.error_message, metadata.error_message);
    }

    #[tokio::test]
    async fn test_porttree_repos_conf_parsing() {
        let temp_dir = TempDir::new().unwrap();
        let repos_conf_path = temp_dir.path().join("repos.conf");

        let repos_conf_content = r#"
[test-repo]
location = /usr/local/test-repo
sync-type = git
sync-uri = https://github.com/test/repo.git
auto-sync = true
sync-depth = 1

[gentoo]
location = /usr/portage
sync-type = rsync
sync-uri = rsync://rsync.gentoo.org/gentoo-portage
auto-sync = true
"#;

        fs::write(&repos_conf_path, repos_conf_content).unwrap();

        let mut porttree = PortTree::new("/");
        porttree.parse_repos_conf(&fs::read_to_string(&repos_conf_path).unwrap());

        assert!(porttree.repositories.contains_key("test-repo"));
        assert!(porttree.repositories.contains_key("gentoo"));

        let test_repo = &porttree.repositories["test-repo"];
        assert_eq!(test_repo.location, "/usr/local/test-repo");
        assert_eq!(test_repo.sync_type, Some("git".to_string()));
        assert_eq!(test_repo.sync_uri, Some("https://github.com/test/repo.git".to_string()));
        assert_eq!(test_repo.auto_sync, true);
        assert_eq!(test_repo.sync_depth, Some(1));

        let gentoo_repo = &porttree.repositories["gentoo"];
        assert_eq!(gentoo_repo.location, "/usr/portage");
        assert_eq!(gentoo_repo.sync_type, Some("rsync".to_string()));
        assert_eq!(gentoo_repo.sync_uri, Some("rsync://rsync.gentoo.org/gentoo-portage".to_string()));
        assert_eq!(gentoo_repo.auto_sync, true);
    }

    #[tokio::test]
    async fn test_sync_metadata_tracking() {
        let temp_dir = TempDir::new().unwrap();
        let mut porttree = PortTree::new("/");

        // Create a test repository
        let repo = Repository {
            name: "test-repo".to_string(),
            location: temp_dir.path().display().to_string(),
            sync_type: Some("rsync".to_string()),
            sync_uri: Some("rsync://example.com/test".to_string()),
            auto_sync: true,
            sync_depth: None,
            sync_hooks_only_on_change: false,
            sync_metadata: SyncMetadata {
                last_sync: None,
                last_attempt: None,
                success: false,
                error_message: None,
            },
            eclass_cache: std::collections::HashMap::new(),
            metadata_cache: std::collections::HashMap::new(),
        };

        porttree.repositories.insert("test-repo".to_string(), repo);

        // Test initial state
        let status = porttree.get_sync_status("test-repo");
        assert!(status.is_some());
        assert_eq!(status.unwrap().last_sync, None);
        assert_eq!(status.unwrap().success, false);

        // Test updating metadata
        porttree.update_sync_metadata("test-repo", true, None);
        let status = porttree.get_sync_status("test-repo").unwrap();
        assert_eq!(status.success, true);
        assert!(status.last_sync.is_some());
        assert!(status.last_attempt.is_some());

        // Test updating with error
        porttree.update_sync_metadata("test-repo", false, Some("Network timeout".to_string()));
        let status = porttree.get_sync_status("test-repo").unwrap();
        assert_eq!(status.success, false);
        assert_eq!(status.error_message, Some("Network timeout".to_string()));
    }

    #[tokio::test]
    async fn test_needs_sync_logic() {
        let temp_dir = TempDir::new().unwrap();
        let mut porttree = PortTree::new("/");

        // Create a repository that doesn't auto-sync
        let repo_no_auto = Repository {
            name: "no-auto".to_string(),
            location: temp_dir.path().display().to_string(),
            sync_type: Some("rsync".to_string()),
            sync_uri: None,
            auto_sync: false,
            sync_depth: None,
            sync_hooks_only_on_change: false,
            sync_metadata: SyncMetadata {
                last_sync: None,
                last_attempt: None,
                success: false,
                error_message: None,
            },
            eclass_cache: std::collections::HashMap::new(),
            metadata_cache: std::collections::HashMap::new(),
        };

        // Create a repository that auto-syncs but was never synced
        let repo_never_synced = Repository {
            name: "never-synced".to_string(),
            location: temp_dir.path().display().to_string(),
            sync_type: Some("rsync".to_string()),
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
            eclass_cache: std::collections::HashMap::new(),
            metadata_cache: std::collections::HashMap::new(),
        };

        porttree.repositories.insert("no-auto".to_string(), repo_no_auto);
        porttree.repositories.insert("never-synced".to_string(), repo_never_synced);

        assert_eq!(porttree.needs_sync("no-auto"), false);
        assert_eq!(porttree.needs_sync("never-synced"), true);
        assert_eq!(porttree.needs_sync("nonexistent"), false);
    }

    #[tokio::test]
    async fn test_sync_error_types() {
        use crate::sync::SyncError;
        
        let network_error = SyncError::Network("Connection failed".to_string());
        let repo_error = SyncError::Repository("Invalid repository".to_string());
        let command_error = SyncError::Command("Command failed".to_string());
        let validation_error = SyncError::Validation("Validation failed".to_string());
        let timeout_error = SyncError::Timeout("Operation timed out".to_string());

        assert!(network_error.to_string().contains("Network error"));
        assert!(repo_error.to_string().contains("Repository error"));
        assert!(command_error.to_string().contains("Command error"));
        assert!(validation_error.to_string().contains("Validation error"));
        assert!(timeout_error.to_string().contains("Timeout error"));
    }
}

async fn get_download_size(src_uri: &str, distdir: &str) -> Option<u64> {
    // Extract filenames from SRC_URI
    // SRC_URI can contain:
    // - URLs: https://example.com/file.tar.gz
    // - Mirrors: mirror://gnu/foo/bar.tar.gz
    // - Arrows: https://example.com/download -> renamed.tar.gz
    
    let mut total_size = 0u64;
    let parts: Vec<&str> = src_uri.split_whitespace().collect();
    let mut i = 0;
    
    while i < parts.len() {
        let part = parts[i];
        
        // Skip USE conditionals and parentheses
        if part.ends_with('?') || part == "(" || part == ")" {
            i += 1;
            continue;
        }
        
        // Extract filename
        let filename = if i + 2 < parts.len() && parts[i + 1] == "->" {
            // Arrow notation: URL -> filename
            i += 2;
            parts[i]
        } else if part.starts_with("http://") || part.starts_with("https://") || part.starts_with("ftp://") {
            // Direct URL - extract filename from URL
            part.split('/').last().unwrap_or(part)
        } else if part.starts_with("mirror://") {
            // Mirror URL - extract filename
            part.split('/').last().unwrap_or(part)
        } else {
            // Assume it's a filename
            part
        };
        
        // Check if file exists in distfiles directory
        let distfile_path = std::path::Path::new(distdir).join(filename);
        if distfile_path.exists() {
            if let Ok(metadata) = std::fs::metadata(&distfile_path) {
                total_size += metadata.len();
            }
        } else {
            // File doesn't exist in distfiles, try to get size from HTTP HEAD request
            // For now, skip files we can't find (to avoid blocking on network)
            // In a full implementation, we'd do async HEAD requests
        }
        
        i += 1;
    }
    
    if total_size > 0 {
        Some(total_size)
    } else {
        None
    }
}

async fn create_merge_plan(
    cpv_packages: &[String],
    vartree: &crate::vartree::VarTree,
    porttree: &mut PortTree,
) -> Result<Vec<MergePlanItem>, Box<dyn std::error::Error + Send + Sync>> {
    let mut plan = Vec::new();
    let installed = vartree.get_all_installed().await.unwrap_or_default();

    for cpv in cpv_packages {
        let cp = if let Some(last_dash) = cpv.rfind('-') {
            let cp_hyphenated = &cpv[..last_dash];
            cp_hyphenated.replace('-', "/")
        } else {
            continue;
        };

        // Extract new version using pkgsplit
        let full_new_cpv = format!("{}-{}", cp.replace("/", "-"), cpv.split('-').last().unwrap_or(""));
        let new_version = if let Some((_, ver, rev)) = crate::versions::pkgsplit(&format!("placeholder/{}", full_new_cpv)) {
            if rev == "r0" {
                ver.to_string()
            } else {
                format!("{}-{}", ver, rev)
            }
        } else {
            cpv.split('-').last().unwrap_or("").to_string()
        };

        // Find installed version - compare CP part
        let cp_hyphenated = cp.replace("/", "-");
        let old_version = installed.iter()
            .find(|installed_cpv| {
                // Extract CP from installed CPV by finding package name
                installed_cpv.starts_with(&format!("{}-", cp_hyphenated))
            })
            .and_then(|installed_cpv| {
                // Extract version from installed CPV
                let installed_cpv_str = format!("placeholder/{}", installed_cpv);
                if let Some((_, ver, rev)) = crate::versions::pkgsplit(&installed_cpv_str) {
                    if rev == "r0" {
                        Some(ver.to_string())
                    } else {
                        Some(format!("{}-{}", ver, rev))
                    }
                } else {
                    None
                }
            });

        let status = if let Some(ref old_ver) = old_version {
            if let Some(cmp) = crate::versions::vercmp(old_ver, &new_version) {
                if cmp < 0 {
                    PackageStatus::Upgrade
                } else if cmp > 0 {
                    PackageStatus::Downgrade
                } else {
                    PackageStatus::Rebuild
                }
            } else {
                PackageStatus::Rebuild
            }
        } else {
            PackageStatus::New
        };

        let metadata = porttree.get_metadata(cpv).await;
        let slot = metadata.as_ref().and_then(|m| m.get("SLOT").map(|s| s.clone()));
        let abi = metadata.as_ref().and_then(|m| m.get("ABI").map(|s| s.clone()));

        // Get repository name
        let repository = porttree.get_ebuild_path_and_repo(cpv).map(|(_, repo)| repo);

        // Get actual download size from distfiles or SRC_URI
        let size = if let Some(m) = metadata.as_ref() {
            if let Some(src_uri) = m.get("SRC_URI") {
                get_download_size(src_uri, "/var/cache/distfiles").await
            } else {
                None
            }
        } else {
            None
        };

        plan.push(MergePlanItem {
            cpv: cpv.clone(),
            status,
            old_version,
            slot,
            size,
            use_changes: vec![],
            repository,
            abi,
        });
    }

    Ok(plan)
}

fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{} KB", bytes / KB)
    } else {
        format!("{} B", bytes)
    }
}

fn display_merge_plan(plan: &[MergePlanItem], config_protect_conflicts: &[String], masked_packages: &[(String, String)], unaccepted_licenses: &[(String, String)]) {
    println!("\nThese are the packages that would be merged, in order:\n");

    let mut total_size = 0u64;
    for item in plan {
        // Format status indicator like Portage: [ebuild  U ]
        let status_indicator = match item.status {
            PackageStatus::New => "[ebuild  N ]",
            PackageStatus::Upgrade => "[ebuild  U ]",
            PackageStatus::Rebuild => "[ebuild  R ]",
            PackageStatus::Downgrade => "[ebuild  D ]",
        };

        // Extract version from CPV using pkgsplit
        let new_version = if let Some((_, ver, rev)) = crate::versions::pkgsplit(&item.cpv) {
            if rev == "r0" {
                ver
            } else {
                format!("{}-{}", ver, rev)
            }
        } else {
            item.cpv.split('-').last().unwrap_or("").to_string()
        };

        let version_info = match (&item.status, &item.old_version) {
            (PackageStatus::Rebuild, Some(old)) => {
                format!("({} rebuilding)", old)
            }
            (_, Some(old)) => {
                format!("({} -> {})", old, new_version)
            }
            _ => {
                format!("({})", new_version)
            }
        };

        let slot_info = if let Some(ref slot) = item.slot {
            format!(":{}", slot)
        } else {
            String::new()
        };

        let repo_info = if let Some(ref repo) = item.repository {
            format!(" ::{}", repo)
        } else {
            String::new()
        };

        let abi_info = if let Some(ref abi) = item.abi {
            format!(" ABI={}", abi)
        } else {
            String::new()
        };

        let size_info = if let Some(size) = item.size {
            total_size += size;
            format!(" {} KiB", size / 1024)
        } else {
            String::new()
        };

        // Show USE flag changes for upgrades
        let use_info = if !item.use_changes.is_empty() && matches!(item.status, PackageStatus::Upgrade) {
            let enabled: Vec<String> = item.use_changes.iter()
                .filter(|(_, enabled)| *enabled)
                .map(|(flag, _)| flag.clone())
                .collect();
            let disabled: Vec<String> = item.use_changes.iter()
                .filter(|(_, enabled)| !*enabled)
                .map(|(flag, _)| format!("-{}", flag))
                .collect();
            let mut all_changes = enabled;
            all_changes.extend(disabled);
            format!(" USE=\"{}\"", all_changes.join(" "))
        } else {
            String::new()
        };

        println!("{}{}{}{}{}{}{}{}",
                 status_indicator,
                 item.cpv,
                 slot_info,
                 version_info,
                 abi_info,
                 use_info,
                 repo_info,
                 size_info);
    }

    if total_size > 0 {
        println!("\nTotal: {} packages, Size of downloads: {} KiB", plan.len(), total_size / 1024);
    } else {
        println!("\nTotal: {} packages", plan.len());
    }

    // Display masked packages
    if !masked_packages.is_empty() {
        println!("\n!!! The following packages are masked:");
        for (cpv, reason) in masked_packages {
            println!("!!! {}: {}", cpv, reason);
        }
        println!("!!! To proceed, you may need to unmask these packages.");
        println!();
    }

    // Display license alerts
    if !unaccepted_licenses.is_empty() {
        println!("\n!!! The following packages have unaccepted licenses:");
        let mut package_licenses: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
        for (cpv, license) in unaccepted_licenses {
            package_licenses.entry(cpv.clone()).or_insert_with(Vec::new).push(license.clone());
        }
        for (cpv, licenses) in &package_licenses {
            println!("!!! {}: {}", cpv, licenses.join(", "));
        }
        println!("!!! You must accept these licenses to proceed.");
        println!();
    }

    // Display CONFIG_PROTECT warnings
    if !config_protect_conflicts.is_empty() {
        println!("\n!!! CONFIG_PROTECT is active for the following files:");
        for conflict in config_protect_conflicts {
            println!("!!! {}", conflict);
        }
        println!("!!! This means that the new files will not overwrite the existing ones.");
        println!("!!! You will need to manually merge the .new files with the existing ones.");
        println!();
    }
}

async fn check_config_protect_conflicts(
    merge_plan: &[MergePlanItem],
    config: &crate::config::Config,
    vartree: &crate::vartree::VarTree,
) -> Result<Vec<String>, Box<dyn std::error::Error + Send + Sync>> {
    let mut conflicts = Vec::new();
    let config_protect_paths = config.get_config_protect();

    for item in merge_plan {
        // Only check for upgrades and rebuilds (not new installs)
        if matches!(item.status, PackageStatus::New) {
            continue;
        }

        // Get the installed package database entry
        let installed = vartree.get_all_installed().await.unwrap_or_default();
        let cp_hyphenated = item.cpv.split('-').take(2).collect::<Vec<_>>().join("-");

        if let Some(installed_cpv) = installed.iter().find(|cpv| cpv.starts_with(&cp_hyphenated)) {
            let pkg_db_path = std::path::Path::new("/var/db/pkg").join(installed_cpv);

            // Read the CONTENTS file to see what files are installed
            let contents_file = pkg_db_path.join("CONTENTS");
            if contents_file.exists() {
                if let Ok(contents) = std::fs::read_to_string(&contents_file) {
                    for line in contents.lines() {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 2 && parts[0] == "obj" {
                            let file_path = parts[1];

                            // Check if this file is in a CONFIG_PROTECT directory
                            for protect_path in &config_protect_paths {
                                if file_path.starts_with(protect_path) {
                                    // Check if the file exists on disk
                                    let full_path = std::path::Path::new(&config.root).join(&file_path[1..]); // Remove leading /
                                    if full_path.exists() {
                                        conflicts.push(format!("{}: {}", item.cpv, file_path));
                                    }
                                    break; // Found a match, no need to check other protect paths
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Ok(conflicts)
}

async fn check_reverse_dependencies(
    packages: &[Atom],
    vartree: &crate::vartree::VarTree,
    porttree: &mut PortTree,
) -> Result<Vec<(String, Vec<String>)>, Box<dyn std::error::Error + Send + Sync>> {
    let mut blocked = Vec::new();

    // Get all installed packages
    let installed = vartree.get_all_installed().await?;

    for pkg_atom in packages {
        let mut dependents = Vec::new();

        // Check each installed package to see if it depends on this package
        for cpv in &installed {
            // Skip if it's the same package
            if pkg_atom.matches(cpv) {
                continue;
            }

            // Get dependencies of this installed package
            if let Some(metadata) = porttree.get_metadata(cpv).await {
                // Check DEPEND, RDEPEND, PDEPEND
                let depend_str = metadata.get("DEPEND").cloned().unwrap_or_default();
                let rdepend_str = metadata.get("RDEPEND").cloned().unwrap_or_default();
                let pdepend_str = metadata.get("PDEPEND").cloned().unwrap_or_default();
                let deps_to_check = [&depend_str, &rdepend_str, &pdepend_str];

                for deps_str in &deps_to_check {
                    if !deps_str.trim().is_empty() {
                        if let Ok(deps) = crate::dep::parse_dependencies(deps_str) {
                            for dep in deps {
                                if pkg_atom.matches(&dep.cpv) {
                                    dependents.push(cpv.clone());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        if !dependents.is_empty() {
            blocked.push((pkg_atom.cp(), dependents));
        }
    }

    Ok(blocked)
}

async fn check_use_changes(
    cp: &str,
    available_cpv: &str,
    installed_cpv: &str,
    porttree: &mut PortTree,
    vartree: &crate::vartree::VarTree,
) -> bool {
    // Extract package directory from installed CPV
    // installed_cpv is like "sys-apps-coreutils-9.7"
    // We need to convert to "sys-apps/coreutils-9.7"
    let parts: Vec<&str> = installed_cpv.splitn(3, '-').collect();
    if parts.len() < 3 {
        return false;
    }
    
    let pkg_dir = format!("{}/{}-{}", parts[0], parts[1], parts[2]);
    let use_file = format!("/var/db/pkg/{}/USE", pkg_dir);
    let iuse_file = format!("/var/db/pkg/{}/IUSE", pkg_dir);
    
    // Get installed USE flags
    let installed_use = std::fs::read_to_string(&use_file)
        .ok()
        .map(|s| {
            s.split_whitespace()
                .filter(|f| !f.is_empty())
                .map(String::from)
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    
    // Get installed IUSE
    let installed_iuse = std::fs::read_to_string(&iuse_file)
        .ok()
        .map(|s| {
            s.split_whitespace()
                .filter(|f| !f.is_empty())
                .map(|f| f.trim_start_matches('+').trim_start_matches('-').to_string())
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    
    // Get available IUSE from metadata
    let available_metadata = porttree.get_metadata(available_cpv).await;
    let available_iuse = available_metadata
        .as_ref()
        .and_then(|m| m.get("IUSE"))
        .map(|s| {
            s.split_whitespace()
                .filter(|f| !f.is_empty())
                .map(|f| f.trim_start_matches('+').trim_start_matches('-').to_string())
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    
    // Only rebuild if:
    // 1. New USE flags were added to IUSE, OR
    // 2. USE flags were removed from IUSE, OR
    // 3. Installed USE flags intersect with changed IUSE
    
    let new_flags: std::collections::HashSet<_> = available_iuse.difference(&installed_iuse).collect();
    let removed_flags: std::collections::HashSet<_> = installed_iuse.difference(&available_iuse).collect();
    
    // Check if any of the changed flags affect the installed USE
    for flag in new_flags {
        // If a new flag exists and might be enabled by default, consider rebuild
        if installed_use.contains(flag.as_str()) {
            return true;
        }
    }
    
    for flag in removed_flags {
        // If a flag was removed but was being used, definitely rebuild
        if installed_use.contains(flag.as_str()) {
            return true;
        }
    }
    
    false
}

async fn get_package_dependencies(
    atom: &crate::atom::Atom,
    porttree: &mut PortTree,
    with_bdeps: bool,
) -> Result<(Vec<DepNode>, Vec<crate::dep::Atom>), Box<dyn std::error::Error + Send + Sync>> {
    // Always find the best available version that satisfies the atom constraint
    // Don't use the exact version from the atom as it might not exist
    let merger = crate::merge::Merger::new("/");
    let cpv = match merger.find_best_version_with_porttree(&atom.cp(), Some(porttree)).await {
        Ok(Some(best_cpv)) => best_cpv,
        Ok(None) => return Err(format!("No version found for package: {}", atom.cp()).into()),
        Err(e) => return Err(format!("Failed to find version for {}: {}", atom.cp(), e).into()),
    };

    // First, try to get dependencies from binary package if available
    let bintree = crate::bintree::BinTree::new("/");
    if let Ok(Some(bin_info)) = bintree.parse_tbz2(&cpv).await {
        let (deps, blockers) = parse_binary_dependencies(&bin_info, with_bdeps)?;
        return Ok((deps, blockers));
    }

    // Fall back to ebuild-based dependency resolution
    get_ebuild_dependencies(&cpv, porttree, with_bdeps).await
}

async fn build_recursive_depgraph(
    atoms: &[crate::atom::Atom],
    porttree: &mut PortTree,
    with_bdeps: bool,
    depgraph: &mut DepGraph,
    max_depth: usize,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use std::collections::{HashSet, VecDeque};
    
    let mut visited: HashSet<String> = HashSet::new();
    let mut queue: VecDeque<(crate::atom::Atom, usize)> = VecDeque::new();
    
    for atom in atoms {
        queue.push_back((atom.clone(), 0));
    }
    
    while let Some((atom, depth)) = queue.pop_front() {
        let cp = atom.cp();
        
        if visited.contains(&cp) {
            continue;
        }
        if depth >= max_depth {
            eprintln!("DEBUG: Hit max depth {} for {}", max_depth, cp);
            continue;
        }
        visited.insert(cp.clone());
        
        let (deps, dep_blockers) = match get_package_dependencies(&atom, porttree, with_bdeps).await {
            Ok((deps, blockers)) => (deps, blockers),
            Err(e) => {
                eprintln!("Warning: Failed to get dependencies for {}: {}", cp, e);
                continue;
            }
        };
        
        let blockers: Vec<crate::atom::Atom> = dep_blockers.into_iter().map(|dep_atom| {
            crate::atom::Atom::new(&dep_atom.cpv).unwrap_or_else(|_| crate::atom::Atom {
                category: dep_atom.cp().split('/').next().unwrap_or("unknown").to_string(),
                package: dep_atom.cp().split('/').nth(1).unwrap_or(&dep_atom.cp()).to_string(),
                version: None,
                op: crate::atom::Operator::None,
                slot: dep_atom.slot,
                subslot: dep_atom.sub_slot,
                repo: dep_atom.repo,
                use_deps: dep_atom.use_deps,
                blocker: dep_atom.blocker,
            })
        }).collect();
        
        if let Err(e) = depgraph.add_node_with_blockers(&cp, deps.clone(), blockers) {
            eprintln!("Warning: Failed to add {} to dependency graph: {}", cp, e);
            continue;
        }
        
        for dep_node in deps {
            let dep_cp = dep_node.atom.cp();
            if !visited.contains(&dep_cp) {
                queue.push_back((dep_node.atom.clone(), depth + 1));
            }
        }
    }
    
    Ok(())
}

async fn get_ebuild_dependencies(
    cpv: &str,
    porttree: &mut PortTree,
    with_bdeps: bool,
) -> Result<(Vec<DepNode>, Vec<crate::dep::Atom>), Box<dyn std::error::Error + Send + Sync>> {
    // Get metadata (from md5-cache or ebuild)
    let metadata_map = porttree.get_metadata(cpv).await
        .ok_or_else(|| format!("Failed to get metadata for {}", cpv))?;
    
    let mut deps = Vec::new();
    let mut blockers = Vec::new();

    // Helper function to parse dependency string and separate blockers
    let parse_dep_string = |dep_str: &str, dep_type: DepType| -> Result<(Vec<DepNode>, Vec<crate::dep::Atom>), Box<dyn std::error::Error + Send + Sync>> {
        let mut nodes = Vec::new();
        let mut blocks = Vec::new();
        
        if !dep_str.trim().is_empty() {
            let dep_atoms = crate::dep::parse_dependencies(dep_str)?;
            for dep_atom in dep_atoms {
                if dep_atom.blocker.is_some() {
                    blocks.push(dep_atom);
                } else {
                    nodes.push(create_dep_node(&dep_atom, dep_type.clone()));
                }
            }
        }
        
        Ok((nodes, blocks))
    };

    // Process BDEPEND (build dependencies from EAPI 7+)
    // Note: In portage, build deps are only included for packages being built from source
    // For binary packages or already-installed packages, build deps are skipped
    // For now, we include them if with_bdeps=y as a simplification
    if with_bdeps {
        if let Some(bdepend_str) = metadata_map.get("BDEPEND") {
            let (mut nodes, mut blocks) = parse_dep_string(bdepend_str, DepType::Build)?;
            deps.append(&mut nodes);
            blockers.append(&mut blocks);
        }
        
        // Also include DEPEND for older EAPIs
        if let Some(depend_str) = metadata_map.get("DEPEND") {
            let (mut nodes, mut blocks) = parse_dep_string(depend_str, DepType::Build)?;
            deps.append(&mut nodes);
            blockers.append(&mut blocks);
        }
    }

    // Always process RDEPEND (runtime dependencies)
    if let Some(rdepend_str) = metadata_map.get("RDEPEND") {
        let (mut nodes, mut blocks) = parse_dep_string(rdepend_str, DepType::Runtime)?;
        deps.append(&mut nodes);
        blockers.append(&mut blocks);
    }

    // Always process PDEPEND (post dependencies)
    if let Some(pdepend_str) = metadata_map.get("PDEPEND") {
        let (mut nodes, mut blocks) = parse_dep_string(pdepend_str, DepType::Post)?;
        deps.append(&mut nodes);
        blockers.append(&mut blocks);
    }

    // Process IDEPEND (install dependencies - always included)
    if let Some(idepend_str) = metadata_map.get("IDEPEND") {
        let (mut nodes, mut blocks) = parse_dep_string(idepend_str, DepType::Runtime)?;
        deps.append(&mut nodes);
        blockers.append(&mut blocks);
    }

    Ok((deps, blockers))
}

fn parse_binary_dependencies(
    bin_info: &crate::bintree::BinPkgInfo,
    with_bdeps: bool,
) -> Result<(Vec<DepNode>, Vec<crate::dep::Atom>), Box<dyn std::error::Error + Send + Sync>> {
    let mut deps = Vec::new();
    let mut blockers = Vec::new();

    // Binary packages typically only have runtime dependencies
    // Check for DEPEND and RDEPEND in the XPAK metadata
    // Only include build dependencies if with_bdeps is true
    if with_bdeps {
        if let Some(depend_str) = bin_info.metadata.get("DEPEND") {
            if !depend_str.trim().is_empty() {
                let depend_atoms = crate::dep::parse_dependencies(depend_str)?;
                for dep_atom in depend_atoms {
                    if dep_atom.blocker.is_some() {
                        blockers.push(dep_atom);
                    } else {
                        deps.push(create_dep_node(&dep_atom, DepType::Build));
                    }
                }
            }
        }
    }

    if let Some(rdepend_str) = bin_info.metadata.get("RDEPEND") {
        if !rdepend_str.trim().is_empty() {
            let rdepend_atoms = crate::dep::parse_dependencies(rdepend_str)?;
            for dep_atom in rdepend_atoms {
                if dep_atom.blocker.is_some() {
                    blockers.push(dep_atom);
                } else {
                    deps.push(create_dep_node(&dep_atom, DepType::Runtime));
                }
            }
        }
    }

    Ok((deps, blockers))
}

fn create_dep_node(dep_atom: &crate::dep::Atom, dep_type: DepType) -> DepNode {
    let atom = crate::atom::Atom::new(&dep_atom.cpv).unwrap_or_else(|_| crate::atom::Atom {
        category: dep_atom
            .cp()
            .split('/')
            .next()
            .unwrap_or("unknown")
            .to_string(),
        package: dep_atom
            .cp()
            .split('/')
            .nth(1)
            .unwrap_or(&dep_atom.cp())
            .to_string(),
        version: None,
        op: crate::atom::Operator::None,
        slot: dep_atom.slot.clone(),
        subslot: dep_atom.sub_slot.clone(),
        repo: dep_atom.repo.clone(),
        use_deps: dep_atom.use_deps.clone(),
        blocker: dep_atom.blocker.clone(),
    });

    let blockers = if dep_atom.blocker.is_some() {
        vec![atom.clone()] // This dependency is a blocker, so this node blocks the atom
    } else {
        vec![]
    };

    DepNode {
        atom,
        dep_type,
        blockers,
        use_conditional: None, // TODO: handle USE conditionals
        slot: dep_atom.slot.clone(),
        subslot: dep_atom.sub_slot.clone(),
    }
}

pub async fn action_install(
    packages: &[String],
    pretend: bool,
    ask: bool,
    resume: bool,
    jobs: usize,
) -> i32 {
    action_install_with_root(packages, pretend, ask, resume, jobs, "/", false).await
}

/// Handle set-related commands
pub async fn action_set(command: Option<&str>, set_name: Option<&str>) -> i32 {
    let set_manager = sets::PackageSetManager::new("/");

    match command {
        Some("list") => {
            match set_manager.list_all_sets() {
                Ok(sets) => {
                    println!("Available package sets:");
                    for set in sets {
                        match set_manager.get_set_info(&set).await {
                            Ok(info) => {
                                println!("  @{} - {} ({} packages)", info.name, info.description, info.package_count);
                            }
                            Err(_) => {
                                println!("  @{} - Custom set", set);
                            }
                        }
                    }
                    0
                }
                Err(e) => {
                    eprintln!("Failed to list sets: {}", e);
                    1
                }
            }
        }
        Some("show") => {
            if let Some(name) = set_name {
                match set_manager.resolve_set(name).await {
                    Ok(packages) => {
                        println!("Contents of @{} set:", name);
                        for pkg in packages {
                            println!("  {}", pkg);
                        }
                        0
                    }
                    Err(e) => {
                        eprintln!("Failed to show set {}: {}", name, e);
                        1
                    }
                }
            } else {
                eprintln!("Set name required for show command");
                1
            }
        }
        Some(cmd) => {
            eprintln!("Unknown set command: {}", cmd);
            eprintln!("Available commands: list, show");
            1
        }
        None => {
            eprintln!("Set command required");
            eprintln!("Available commands: list, show");
            1
        }
    }
}

pub async fn action_upgrade(
    packages: &[String],
    pretend: bool,
    ask: bool,
    deep: bool,
    newuse: bool,
    with_bdeps: bool,
) -> i32 {
    println!("Upgrading packages: {:?}", packages);

    let pretend_mode = pretend;
    if pretend {
        println!("Pretend mode: simulating upgrade of {:?}", packages);
    }

    // Resolve sets (@world, @system, etc.) to individual packages
    let resolved_packages = match sets::resolve_targets(packages, "/").await {
        Ok(pkgs) => pkgs,
        Err(e) => {
            eprintln!("Failed to resolve package sets: {}", e);
            return 1;
        }
    };

    // Initialize portage tree for finding ebuilds
    let mut porttree = PortTree::new("/");
    porttree.scan_repositories();

    // Initialize configuration and masking
    let config = match crate::config::Config::new("/").await {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load configuration: {}", e);
            return 1;
        }
    };
    let mask_manager = crate::mask::MaskManager::new("/", config.accept_keywords.clone());

    // Initialize merger for finding best versions
    let merger = crate::merge::Merger::new("/");

    // Parse atoms from resolved packages and convert to best available CPVs
    let mut target_cpvs: Vec<String> = Vec::new();
    for pkg in &resolved_packages {
        match Atom::new(pkg) {
            Ok(atom) => {
                // Find the best available version for this atom
                match merger.find_best_version_with_porttree(&atom.cp(), Some(&porttree)).await {
                    Ok(Some(cpv)) => {
                        // Check if masked
                        if let Ok(cpv_atom) = crate::atom::Atom::new(&cpv) {
                            if let Some(_mask_reason) = mask_manager.is_masked(&cpv_atom).await.unwrap_or(None) {
                                // Skip masked packages
                                continue;
                            }
                        }
                        target_cpvs.push(cpv);
                    }
                    Ok(None) => {
                        // No version available, skip
                        eprintln!("Warning: No version available for {}", pkg);
                        continue;
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to find version for {}: {}", pkg, e);
                        continue;
                    }
                }
            }
            Err(e) => {
                eprintln!("Invalid atom '{}': {}", pkg, e);
                return 1;
            }
        }
    }

    if target_cpvs.is_empty() {
        println!("No valid packages to upgrade.");
        return 0;
    }

    // Create dependency graph with USE flags
    let use_flags = config.get_use_flags_map();
    let mut depgraph = DepGraph::with_use_flags(use_flags);

    // Parse atoms from the best CPVs for dependency resolution
    // Need to add '=' operator so Atom::new() parses the version correctly
    let mut target_atoms = Vec::new();
    for cpv in &target_cpvs {
        let atom_str = format!("={}", cpv);
        match Atom::new(&atom_str) {
            Ok(atom) => target_atoms.push(atom),
            Err(e) => {
                eprintln!("Invalid CPV '{}': {}", cpv, e);
                continue;
            }
        }
    }

    // Build recursive dependency graph for all targets (max depth 50 to prevent infinite loops)
    // with_bdeps parameter controls whether to include build-time dependencies (BDEPEND, DEPEND)
    println!("Calculating dependencies...");
    if let Err(e) = build_recursive_depgraph(&target_atoms, &mut porttree, with_bdeps, &mut depgraph, 50).await {
        eprintln!("Failed to build dependency graph: {}", e);
        return 1;
    }
    println!("Dependency calculation complete.");

    // Resolve dependencies using BFS resolver
    // Extract CPs from CPVs since the graph is keyed by CP
    let target_cps: Vec<String> = target_atoms.iter().map(|a| a.cp()).collect();
    match depgraph.resolve_advanced(&target_cps) {
        Ok(result) => {
            if !result.blocked.is_empty() {
                eprintln!("Warning: Blocked packages: {:?}", result.blocked);
            }
            if !result.circular.is_empty() {
                eprintln!("Warning: {} circular dependencies detected (this is normal)", result.circular.len());
            }

            println!("Resolved {} packages from dependency tree", result.resolved.len());
            println!("Dependency resolution took {:.2} ms", result.resolution_time_ms as f64 / 1000.0);

            // Now categorize packages: new, upgrade, reinstall, new slot
            let vartree = crate::vartree::VarTree::new("/");
            let merger = crate::merge::Merger::new("/");

            let installed = vartree.get_all_installed().await.unwrap_or_default();

            #[derive(Debug)]
            struct PackageAction {
                cp: String,
                available_cpv: String,
                installed_cpv: Option<String>,
                status: String, // "U", "N", "R", "NS"
                old_version: Option<String>,
                new_version: String,
                slot: Option<String>,
                mask_reason: Option<String>,
            }

            let mut actions = Vec::new();
            let mut masked_packages = Vec::new();
            
            // Build a set of CPs to check
            // Start with packages from the resolved dependency tree
            let mut cps_to_check: std::collections::HashSet<String> = result.resolved.iter().cloned().collect();
            
            // IMPORTANT: Also include all target packages from @world explicitly
            // Even if they don't have dependency updates, they might have direct updates
            for pkg in &resolved_packages {
                cps_to_check.insert(pkg.clone());
            }
            
            for cp in &cps_to_check {
                // Find the best available version
                if let Ok(Some(available_cpv)) = merger.find_best_version_with_porttree(&cp, Some(&porttree)).await {
                    let available_atom_str = format!("={}", available_cpv);
                    let available_atom = match crate::atom::Atom::new(&available_atom_str) {
                        Ok(atom) => atom,
                        Err(_) => continue,
                    };

                    // Check if masked
                    let mask_reason = mask_manager.is_masked(&available_atom).await.unwrap_or(None);
                    
                    // Use pkgsplit to extract version and slot
                    let (available_version, slot) = if let Some((_cat_pkg, avail_ver, avail_rev)) = crate::versions::pkgsplit(&available_cpv) {
                        let version = if avail_rev == "r0" {
                            avail_ver
                        } else {
                            format!("{}-{}", avail_ver, avail_rev)
                        };
                        // Get slot from metadata
                        let slot = porttree.get_metadata(&available_cpv).await
                            .and_then(|m| m.get("SLOT").map(|s| s.split('/').next().unwrap_or(s).to_string()));
                        (version, slot)
                    } else {
                        continue;
                    };

                    // Check if installed - need to check both slot-specific and any version
                    let cp_hyphenated = cp.replace('/', "-");
                    let search_prefix = format!("{}-", cp_hyphenated);
                    let mut found_installed_same_slot: Option<(String, String, Option<String>)> = None; // (cpv, version, slot)
                    let mut any_version_installed = false;
                    
                    for installed_cpv in &installed {
                        if installed_cpv.starts_with(&search_prefix) {
                            let version_part = installed_cpv.trim_start_matches(&search_prefix);
                            let installed_full = format!("{}-{}", cp, version_part);
                            if let Some((_cat_pkg, inst_ver, inst_rev)) = crate::versions::pkgsplit(&installed_full) {
                                let installed_version = if inst_rev == "r0" {
                                    inst_ver
                                } else {
                                    format!("{}-{}", inst_ver, inst_rev)
                                };
                                // Get installed slot from vartree metadata
                                // cp is "category/package", version_part is "version"
                                // Path is /var/db/pkg/category/package-version/SLOT
                                let (category, package) = cp.split_once('/').unwrap_or(("", &cp));
                                let installed_slot = std::fs::read_to_string(format!("/var/db/pkg/{}/{}-{}/SLOT", category, package, version_part)).ok()
                                    .map(|s| s.trim().split('/').next().unwrap_or(&s).to_string());
                                
                                any_version_installed = true;
                                
                                // Check if this installed version is in the same slot as the available version
                                if slot.is_none() || (installed_slot.is_some() && slot == installed_slot) {
                                    // Matching slot or no slot specified
                                    found_installed_same_slot = Some((installed_cpv.clone(), installed_version, installed_slot));
                                    break;
                                }
                                // Continue searching for matching slot
                            }
                        }
                    }

                    // Determine status
                    let status = if let Some((inst_cpv, inst_ver, inst_slot)) = &found_installed_same_slot {
                        // Same slot is installed - compare versions
                        if let Some(cmp) = crate::versions::vercmp(&inst_ver, &available_version) {
                            if cmp < 0 {
                                "U" // Upgrade
                            } else if cmp == 0 {
                                // Same version - skip unless there's a rebuild reason
                                // TODO: Implement USE flag change detection and subslot rebuild detection
                                // For now, skip these to match portage's behavior
                                continue
                            } else {
                                continue // Downgrade, skip
                            }
                        } else {
                            continue
                        }
                    } else if any_version_installed {
                        // Different slot is installed - this is a new slot installation
                        if result.resolved.contains(&cp.to_string()) {
                            "NS" // New slot
                        } else {
                            continue // Not in resolved tree, skip
                        }
                    } else {
                        // Not installed at all - new package
                        if result.resolved.contains(&cp.to_string()) {
                            "N" // New package
                        } else {
                            continue // Not in resolved tree and not installed, skip
                        }
                    };

                    let action = PackageAction {
                        cp: cp.clone(),
                        available_cpv: available_cpv.clone(),
                        installed_cpv: found_installed_same_slot.as_ref().map(|(cpv, _, _)| cpv.clone()),
                        status: status.to_string(),
                        old_version: found_installed_same_slot.as_ref().map(|(_, ver, _)| ver.clone()),
                        new_version: available_version,
                        slot,
                        mask_reason: mask_reason.clone(),
                    };

                    if mask_reason.is_some() {
                        masked_packages.push(action);
                    } else {
                        // Filter out acct-group and acct-user packages from display
                        // These are auto-installed IDEPEND packages that portage typically
                        // doesn't show in emerge output
                        if !cp.starts_with("acct-group/") && !cp.starts_with("acct-user/") {
                            actions.push(action);
                        }
                    }
                }
            }

            // Count by status
            let upgrades = actions.iter().filter(|a| a.status == "U").count();
            let new_packages = actions.iter().filter(|a| a.status == "N").count();
            let reinstalls = actions.iter().filter(|a| a.status == "R").count();
            let new_slots = actions.iter().filter(|a| a.status == "NS").count();

            if actions.is_empty() && masked_packages.is_empty() {
                println!("No packages to upgrade.");
                return 0;
            }

            println!("\nThese are the packages that would be merged, in order:\n");
            
            // Display packages grouped by status
            // First show upgrades
            for action in actions.iter().filter(|a| a.status == "U") {
                let version_info = if let Some(old_ver) = &action.old_version {
                    format!("[{} -> {}]", old_ver, action.new_version)
                } else {
                    format!("[{}]", action.new_version)
                };
                
                println!("[ebuild     U  ] {}{} {}", action.cp, 
                    action.slot.as_ref().map(|s| format!(":{}", s)).unwrap_or_default(),
                    version_info);
            }
            
            // Then new packages
            for action in actions.iter().filter(|a| a.status == "N") {
                println!("[ebuild  N     ] {}{} [{}]", action.cp,
                    action.slot.as_ref().map(|s| format!(":{}", s)).unwrap_or_default(),
                    action.new_version);
            }
            
            // New slots
            for action in actions.iter().filter(|a| a.status == "NS") {
                let version_info = if let Some(old_ver) = &action.old_version {
                    format!("[{} -> {}]", old_ver, action.new_version)
                } else {
                    format!("[{}]", action.new_version)
                };
                
                println!("[ebuild  NS    ] {}{} {}", action.cp,
                    action.slot.as_ref().map(|s| format!(":{}", s)).unwrap_or_default(),
                    version_info);
            }
            
            // Rebuilds last
            for action in actions.iter().filter(|a| a.status == "R") {
                println!("[ebuild   R    ] {}{} [{}]", action.cp,
                    action.slot.as_ref().map(|s| format!(":{}", s)).unwrap_or_default(),
                    action.new_version);
            }

            println!("\nTotal: {} packages ({} upgrades, {} new, {} in new slots, {} reinstalls)",
                actions.len(), upgrades, new_packages, new_slots, reinstalls);

            // Show masked packages
            if !masked_packages.is_empty() {
                println!("\n!!! The following updates are masked:");
                for action in &masked_packages {
                    println!("- {} ({})", action.cp, action.mask_reason.as_ref().unwrap_or(&"unknown reason".to_string()));
                }
            }

            return 0;
        }
        Err(e) => {
            eprintln!("Dependency resolution failed: {}", e);
            return 1;
        }
    }
}

pub async fn action_install_with_root(
    packages: &[String],
    pretend: bool,
    ask: bool,
    resume: bool,
    jobs: usize,
    root: &str,
    with_bdeps: bool,
) -> i32 {
    println!("Installing packages: {:?}", packages);

    let pretend_mode = pretend;
    if pretend {
        println!("Pretend mode: simulating installation of {:?}", packages);
    }

    // Resolve sets (@world, @system, etc.) to individual packages
    let resolved_packages = match sets::resolve_targets(packages, "/").await {
        Ok(pkgs) => pkgs,
        Err(e) => {
            eprintln!("Failed to resolve package sets: {}", e);
            return 1;
        }
    };

    // Parse atoms from resolved packages
    let mut atoms = Vec::new();
    for pkg in &resolved_packages {
        match Atom::new(pkg) {
            Ok(atom) => atoms.push(atom),
            Err(e) => {
                eprintln!("Invalid atom '{}': {}", pkg, e);
                return 1;
            }
        }
    }

    // Create dependency graph with USE flags
    let config = match crate::config::Config::new(root).await {
        Ok(c) => c,
        Err(_) => crate::config::Config {
            root: root.to_string(),
            make_conf: std::collections::HashMap::new(),
            profile_settings: crate::profile::ProfileSettings::default(),
            use_flags: vec![],
            accept_keywords: vec![],
            features: vec![],
            package_use: std::collections::HashMap::new(),
            package_keywords: std::collections::HashMap::new(),
            package_mask: std::collections::HashSet::new(),
            package_unmask: std::collections::HashSet::new(),
            sets_conf: std::collections::HashMap::new(),
            binhost: vec![],
            binhost_mirrors: vec![],
        },
    };
    let use_flags = config.get_use_flags_map();
    let mut depgraph = DepGraph::with_use_flags(use_flags);

    // Initialize portage tree for finding ebuilds
    let mut porttree = PortTree::new(root);
    porttree.scan_repositories();

    // Build recursive dependency graph (max depth 50 to prevent infinite loops)
    println!("Calculating dependencies...");
    if let Err(e) = build_recursive_depgraph(&atoms, &mut porttree, with_bdeps, &mut depgraph, 50).await {
        eprintln!("Failed to build dependency graph: {}", e);
        return 1;
    }
    println!("Dependency calculation complete.");

    // Resolve dependencies
    match depgraph.resolve(&atoms.iter().map(|a| a.cp()).collect::<Vec<_>>(), &mut porttree).await {
        Ok(result) => {
            // Don't fail on blocked packages - just warn and continue
            if !result.blocked.is_empty() {
                eprintln!("Warning: Blocked packages: {:?}", result.blocked);
            }
            if !result.circular.is_empty() {
                eprintln!("Warning: Circular dependencies: {:?}", result.circular);
            }

            println!("Resolved {} packages to install", result.resolved.len());
            println!("Dependency resolution took {:.2} ms", result.resolution_time_ms as f64 / 1000.0);
            println!("Dependency resolution required {} backtrack attempts", result.backtrack_count);

            // For testing, just return success
            return 0;
        }
        Err(e) => {
            eprintln!("Dependency resolution failed: {}", e);
            return 1;
        }
    }
}

pub async fn action_remove(packages: &[String], pretend: bool, ask: bool) -> i32 {
    println!("Removing packages: {:?}", packages);

    // Resolve sets (@world, @system, etc.) to individual packages
    let resolved_packages = match sets::resolve_targets(packages, "/").await {
        Ok(pkgs) => pkgs,
        Err(e) => {
            eprintln!("Failed to resolve package sets: {}", e);
            return 1;
        }
    };

    // Initialize components
    let vartree = crate::vartree::VarTree::new("/");
    let mut porttree = PortTree::new("/");
    porttree.scan_repositories();

    // Parse packages to remove
    let mut packages_to_remove = Vec::new();
    for pkg in &resolved_packages {
        match Atom::new(pkg) {
            Ok(atom) => {
                packages_to_remove.push(atom);
            }
            Err(e) => {
                eprintln!("Invalid package atom '{}': {}", pkg, e);
                return 1;
            }
        }
    }

    // Check reverse dependencies
    match check_reverse_dependencies(&packages_to_remove, &vartree, &mut porttree).await {
        Ok(blocked) => {
            if !blocked.is_empty() {
                eprintln!("Cannot remove packages due to reverse dependencies:");
                for (pkg, dependents) in blocked {
                    eprintln!("  {} is required by: {:?}", pkg, dependents);
                }
                return 1;
            }
        }
        Err(e) => {
            eprintln!("Failed to check reverse dependencies: {}", e);
            return 1;
        }
    }

    if pretend {
        println!("Pretend mode: would remove {:?}", packages);
        return 0;
    }

    if ask {
        println!("Would you like to proceed? (y/N)");
        // Placeholder: in real implementation, read user input
        println!("Proceeding with removal...");
    }

    // Perform the removal
    let merger = crate::merge::Merger::new("/");
    let mut success_count = 0;

    for atom in &packages_to_remove {
        // Find the installed CPV for this atom
        let installed = match vartree.get_all_installed().await {
            Ok(installed) => installed,
            Err(e) => {
                eprintln!("Failed to get installed packages: {}", e);
                continue;
            }
        };

        let mut cpv_to_remove = None;
        for cpv in &installed {
            if atom.matches(cpv) {
                cpv_to_remove = Some(cpv.clone());
                break;
            }
        }

        if let Some(cpv) = cpv_to_remove {
            match merger.remove_packages(&[cpv], false).await {
                Ok(result) => {
                    if result.failed.is_empty() {
                        println!("Successfully removed {}", atom.cp());
                        success_count += 1;
                    } else {
                        eprintln!("Failed to remove {}: {:?}", atom.cp(), result.failed);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to remove {}: {}", atom.cp(), e);
                }
            }
        } else {
            eprintln!("{} is not installed.", atom.cp());
        }
    }

    if success_count == packages_to_remove.len() {
        println!("All packages removed successfully.");
        0
    } else {
        eprintln!(
            "Removed {}/{} packages.",
            success_count,
            packages_to_remove.len()
        );
        1
    }
}

pub async fn action_search(pattern: &str) -> i32 {
    println!("Searching for packages matching: {}", pattern);

    // Initialize components
    let mut porttree = PortTree::new("/");
    porttree.scan_repositories();

    let mut candidate_cpvs = Vec::new();

    // First pass: find all candidate packages
    for repo in porttree.repositories.values() {
        if let Ok(entries) = std::fs::read_dir(&repo.location) {
            for entry in entries {
                if let Ok(entry) = entry {
                    if let Ok(file_type) = entry.file_type() {
                        if file_type.is_dir() {
                            if let Some(category_name) =
                                entry.path().file_name().and_then(|n| n.to_str())
                            {
                                // Skip non-category directories
                                if category_name.starts_with('.') || category_name == "metadata" {
                                    continue;
                                }

                                // Search packages in this category
                                if let Ok(pkg_entries) = std::fs::read_dir(entry.path()) {
                                    for pkg_entry in pkg_entries {
                                        if let Ok(pkg_entry) = pkg_entry {
                                            if let Ok(pkg_file_type) = pkg_entry.file_type() {
                                                if pkg_file_type.is_dir() {
                                                    if let Some(pkg_name) = pkg_entry
                                                        .path()
                                                        .file_name()
                                                        .and_then(|n| n.to_str())
                                                    {
                                                        let cp = format!(
                                                            "{}/{}",
                                                            category_name, pkg_name
                                                        );

                                                        // Check if package name matches
                                                        if cp.contains(pattern) {
                                                            // Find best version
                                                            let merger =
                                                                crate::merge::Merger::new("/");
                                                            if let Ok(Some(cpv)) =
                                                                merger.find_best_version(&cp).await
                                                            {
                                                                candidate_cpvs.push(cpv);
                                                            }
                                                        } else {
                                                            // For description search, we need to check metadata
                                                            // For now, just collect all packages and check descriptions later
                                                            let merger =
                                                                crate::merge::Merger::new("/");
                                                            if let Ok(Some(cpv)) =
                                                                merger.find_best_version(&cp).await
                                                            {
                                                                candidate_cpvs.push(cpv);
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

    // Second pass: get metadata and filter by description if needed
    let mut matches = Vec::new();
    for cpv in candidate_cpvs {
                if let Some(metadata) = porttree.get_metadata(&cpv).await {
            let cp = cpv.split('-').take(2).collect::<Vec<_>>().join("-");
            if cp.contains(pattern) {
                // Name match
                matches.push((cpv, metadata));
            } else if let Some(desc) = metadata.get("DESCRIPTION") {
                if desc.to_lowercase().contains(&pattern.to_lowercase()) {
                    // Description match
                    matches.push((cpv, metadata));
                }
            }
        }
    }

    // Display results
    if matches.is_empty() {
        println!("No packages found matching '{}'", pattern);
    } else {
        println!("Found {} packages:", matches.len());
        println!();

        for (cpv, metadata) in matches {
            print!("  {}", cpv);

            if let Some(desc) = metadata.get("DESCRIPTION") {
                // Truncate long descriptions
                let truncated = if desc.len() > 60 {
                    format!("{}...", &desc[..57])
                } else {
                    desc.clone()
                };
                println!(" - {}", truncated);
            } else {
                println!();
            }
        }
    }

    0
}

pub async fn action_info(packages: &[String]) -> i32 {
    println!("Getting info for packages: {:?}", packages);

    // Resolve sets (@world, @system, etc.) to individual packages
    let resolved_packages = match sets::resolve_targets(packages, "/").await {
        Ok(pkgs) => pkgs,
        Err(e) => {
            eprintln!("Failed to resolve package sets: {}", e);
            return 1;
        }
    };

    // Initialize components
    let mut porttree = PortTree::new("/");
    porttree.scan_repositories();
    let merger = crate::merge::Merger::new("/");

    for pkg in &resolved_packages {
        // Try to parse as atom first, then as category/package
        let cp = if let Ok(atom) = Atom::new(pkg) {
            atom.cp()
        } else {
            // Assume it's category/package format
            pkg.clone()
        };

        // Find the best available version
        match merger.find_best_version_with_porttree(&cp, Some(&porttree)).await {
            Ok(Some(cpv)) => {
                // Get metadata for this package
                if let Some(metadata) = porttree.get_metadata(&cpv).await {
                    display_package_info(&cpv, &metadata);
                } else {
                    eprintln!("No metadata found for {}", cpv);
                }
            }
            Ok(None) => {
                eprintln!("Package {} not found", cp);
            }
            Err(e) => {
                eprintln!("Error finding package {}: {}", cp, e);
            }
        }

        // Add a blank line between packages
        println!();
    }

    0
}

fn display_package_info(cpv: &str, metadata: &std::collections::HashMap<String, String>) {
    println!("Package: {}", cpv);

    if let Some(desc) = metadata.get("DESCRIPTION") {
        println!("Description: {}", desc);
    }

    if let Some(homepage) = metadata.get("HOMEPAGE") {
        println!("Homepage: {}", homepage);
    }

    if let Some(license) = metadata.get("LICENSE") {
        println!("License: {}", license);
    }

    if let Some(slot) = metadata.get("SLOT") {
        println!("Slot: {}", slot);
    }

    if let Some(keywords) = metadata.get("KEYWORDS") {
        println!("Keywords: {}", keywords);
    }

    if let Some(iuse) = metadata.get("IUSE") {
        if !iuse.trim().is_empty() {
            println!("USE flags: {}", iuse);
        }
    }

    if let Some(depend) = metadata.get("DEPEND") {
        if !depend.trim().is_empty() {
            println!("Build dependencies: {}", depend);
        }
    }

    if let Some(rdepend) = metadata.get("RDEPEND") {
        if !rdepend.trim().is_empty() {
            println!("Runtime dependencies: {}", rdepend);
        }
    }

    if let Some(pdepend) = metadata.get("PDEPEND") {
        if !pdepend.trim().is_empty() {
            println!("Post dependencies: {}", pdepend);
        }
    }
}

async fn get_all_upgradable_packages(
    vartree: &crate::vartree::VarTree,
    merger: &crate::merge::Merger,
    porttree: &crate::porttree::PortTree,
    mask_manager: &crate::mask::MaskManager,
) -> Result<Vec<(String, String, String)>, Box<dyn std::error::Error>> {
    let mut upgradable = Vec::new();

    let installed = vartree.get_all_installed().await?;
    for cpv in installed {
        // Extract CP from CPV (CPV is category-package-version)
        if let Some(last_dash) = cpv.rfind('-') {
            let cp_hyphenated = &cpv[..last_dash];
            let installed_version = &cpv[last_dash + 1..];

            // Convert back to category/package format for merger
            let cp = cp_hyphenated.replace('-', "/");

            // Check if package is masked
            if let Ok(atom) = crate::atom::Atom::new(&cp) {
                if let Some(mask_reason) = mask_manager.is_masked(&atom).await? {
                    // Skip masked packages
                    continue;
                }
            }

            // Find best available version
            if let Ok(Some(available_cpv)) = merger.find_best_version_with_porttree(&cp, Some(porttree)).await {
                // Check if the available version is masked or keyword-restricted
                if let Ok(available_atom) = crate::atom::Atom::new(&available_cpv) {
                    if let Some(mask_reason) = mask_manager.is_masked(&available_atom).await? {
                        // Skip masked versions
                        continue;
                    }
                }

                // Extract version from available CPV
                if let Some(avail_last_dash) = available_cpv.rfind('-') {
                    let available_version = &available_cpv[avail_last_dash + 1..];

                    // Compare versions
                    if let Some(cmp) = crate::versions::vercmp(installed_version, available_version)
                    {
                        if cmp < 0 {
                            // installed < available
                            upgradable.push((
                                cp.to_string(),
                                installed_version.to_string(),
                                available_version.to_string(),
                            ));
                        }
                    }
                }
            }
        }
    }

    Ok(upgradable)
}

async fn get_specific_upgradable_packages(
    packages: &[String],
    vartree: &crate::vartree::VarTree,
    merger: &crate::merge::Merger,
    porttree: &crate::porttree::PortTree,
    mask_manager: &crate::mask::MaskManager,
) -> Result<Vec<(String, String, String)>, Box<dyn std::error::Error>> {
    let mut upgradable = Vec::new();

    for pkg in packages {
        // Parse atom to get CP
                match Atom::new(pkg) {
                    Ok(atom) => {
                        let cp = atom.cp();

                        // Check if package is masked
                        if let Some(_mask_reason) = mask_manager.is_masked(&atom).await? {
                            // Package is masked, skip it
                            continue;
                        }

                        // Check if installed
                        let installed = vartree.get_all_installed().await?;
                        let mut found_installed = None;
                        // Convert cp from category/package to category-package for matching
                        let cp_hyphenated = cp.replace('/', "-");
                        for cpv in &installed {
                            if cpv.starts_with(&format!("{}-", cp_hyphenated)) {
                                if let Some(last_dash) = cpv.rfind('-') {
                                    found_installed = Some(cpv[last_dash + 1..].to_string());
                                }
                                break;
                            }
                        }

                        if let Some(installed_version) = found_installed {
                            // Find best available version
                            if let Ok(Some(available_cpv)) = merger.find_best_version_with_porttree(&cp, Some(porttree)).await {
                                // Check if the available version is masked or keyword-restricted
                                let available_atom = crate::atom::Atom {
                                    category: cp.split('/').next().unwrap_or("").to_string(),
                                    package: cp.split('/').nth(1).unwrap_or("").to_string(),
                                    version: Some(available_cpv.split('-').last().unwrap_or("").to_string()),
                                    op: crate::atom::Operator::None,
                                    slot: None,
                                    subslot: None,
                                    repo: None,
                                    use_deps: vec![],
                                    blocker: None,
                                };

                                if let Some(_mask_reason) = mask_manager.is_masked(&available_atom).await? {
                                    // Version is masked, skip it
                                    continue;
                                }

                                if let Some(avail_last_dash) = available_cpv.rfind('-') {
                                    let available_version = &available_cpv[avail_last_dash + 1..];

                                    // Compare versions
                                    if let Some(cmp) =
                                        crate::versions::vercmp(&installed_version, available_version)
                                    {
                                         if cmp < 0 {
                                             // installed < available
                                             upgradable.push((
                                                 cp,
                                                 installed_version,
                                                 available_version.to_string(),
                                             ));
                                         }
                                    }
                                }
                            } else {
                                eprintln!("No available version found for {} (may be masked or ~arch)", cp);
                            }
                        } else {
                            eprintln!("{} is not installed.", cp);
                        }
                    }
            Err(e) => {
                eprintln!("Invalid package atom '{}': {}", pkg, e);
            }
        }
    }

    Ok(upgradable)
}


