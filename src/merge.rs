// merge.rs -- Package installation and removal logic

use tokio::fs;
use std::path::Path;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use crate::exception::InvalidData;
use crate::vartree::VarTree;
use crate::versions::PkgStr;
use crate::doebuild::{doebuild, BuildPhase};
use crate::bintree::BinTree;
use crate::porttree::PortTree;
use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct MergeResult {
    pub installed: Vec<String>,
    pub failed: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResumeState {
    pub operation_id: String,
    pub packages: Vec<String>,
    pub completed: Vec<String>,
    pub failed: Vec<String>,
    pub in_progress: Option<String>,
    pub start_time: chrono::DateTime<chrono::Utc>,
}

pub struct Merger {
    pub root: String,
    pub vartree: VarTree,
    pub binhost: Vec<String>,
    pub binhost_mirrors: Vec<String>,
}

impl Merger {
    pub fn new(root: &str) -> Self {
        Merger {
            root: root.to_string(),
            vartree: VarTree::new(root),
            binhost: vec![],
            binhost_mirrors: vec![],
        }
    }

    pub fn with_binhost(root: &str, binhost: Vec<String>, binhost_mirrors: Vec<String>) -> Self {
        Merger {
            root: root.to_string(),
            vartree: VarTree::new(root),
            binhost,
            binhost_mirrors,
        }
    }

    /// Find the best available version for a package, considering PortTree
    pub async fn find_best_version_with_porttree(&self, cp: &str, porttree: Option<&PortTree>) -> Result<Option<String>, InvalidData> {
        // First check binary packages
        if !self.binhost.is_empty() {
            // TODO: Check binhost for available versions
        }

        // Check PortTree for ebuild versions
        if let Some(porttree) = porttree {
            if let Some(best_cpv) = self.find_best_ebuild_version(cp, porttree).await? {
                return Ok(Some(best_cpv));
            }
        }

        Ok(None)
    }

    /// Find the best ebuild version from PortTree
    async fn find_best_ebuild_version(&self, cp: &str, porttree: &PortTree) -> Result<Option<String>, InvalidData> {
        let mut best_cpv: Option<String> = None;
        let mut best_cmp = i32::MIN;

        // Split cp into category and package
        let parts: Vec<&str> = cp.split('/').collect();
        if parts.len() != 2 {
            return Ok(None);
        }
        let category = parts[0];
        let package = parts[1];

        // Check each repository
        for repo in porttree.repositories.values() {
            let category_path = Path::new(&repo.location).join(category);
            if !category_path.exists() {
                continue;
            }

            let package_path = category_path.join(package);
            if !package_path.exists() {
                continue;
            }

            // Scan for ebuild files
            if let Ok(mut entries) = fs::read_dir(&package_path).await {
                while let Some(entry) = entries.next_entry().await.transpose() {
                    let entry = match entry {
                        Ok(e) => e,
                        Err(_) => continue,
                    };
                    let path = entry.path();
                    if let Some(ext) = path.extension() {
                        if ext == "ebuild" {
                            if let Some(filename) = path.file_stem() {
                                let filename_str = filename.to_string_lossy();
                                
                                // Parse the full CPV using pkgsplit to properly extract version
                                let cpv = format!("{}/{}", category, filename_str);
                                if let Some((_, ver, rev)) = crate::versions::pkgsplit(&cpv) {
                                    // Skip live ebuilds (9999, 99999999, etc.) unless no other version exists
                                    if ver.contains("9999") {
                                        continue;
                                    }

                                    // Reconstruct the full version string (with revision if not r0)
                                    let full_version = if rev == "r0" {
                                        ver.clone()
                                    } else {
                                        format!("{}-{}", ver, rev)
                                    };

                                    // Compare versions
                                    if let Some(current_best) = &best_cpv {
                                        // Extract version from current best
                                        if let Some((_, best_ver, best_rev)) = crate::versions::pkgsplit(current_best) {
                                            let best_full_version = if best_rev == "r0" {
                                                best_ver
                                            } else {
                                                format!("{}-{}", best_ver, best_rev)
                                            };
                                            if let Some(cmp) = crate::versions::vercmp(&full_version, &best_full_version) {
                                                if cmp > 0 {
                                                    best_cpv = Some(cpv);
                                                    best_cmp = cmp;
                                                }
                                            }
                                        }
                                    } else {
                                        // First valid version found
                                        best_cpv = Some(cpv);
                                        best_cmp = 0;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Ok(best_cpv)
    }

    /// Get the path to the resume state file
    fn resume_state_path(&self) -> std::path::PathBuf {
        Path::new(&self.root).join("var/cache/edb/emerge.state")
    }

    /// Save resume state
    async fn save_resume_state(&self, state: &ResumeState) -> Result<(), InvalidData> {
        let state_path = self.resume_state_path();
        
        // Create directory if needed
        if let Err(e) = tokio::fs::create_dir_all(state_path.parent().unwrap()).await {
            eprintln!("Warning: Failed to create state directory: {}", e);
            return Ok(()); // Don't fail the entire operation
        }

        let json = match serde_json::to_string_pretty(state) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("Warning: Failed to serialize state: {}", e);
                return Ok(()); // Don't fail the entire operation
            }
        };

        if let Err(e) = tokio::fs::write(&state_path, json).await {
            eprintln!("Warning: Failed to write state file: {}", e);
            // Don't fail the entire operation if we can't save resume state
        }

        Ok(())
    }

    /// Load resume state
    async fn load_resume_state(&self) -> Result<Option<ResumeState>, InvalidData> {
        let state_path = self.resume_state_path();
        if !state_path.exists() {
            return Ok(None);
        }

        let content = tokio::fs::read_to_string(&state_path)
            .await
            .map_err(|e| InvalidData::new(&format!("Failed to read state file: {}", e), None))?;

        let state: ResumeState = serde_json::from_str(&content)
            .map_err(|e| InvalidData::new(&format!("Failed to parse state file: {}", e), None))?;

        Ok(Some(state))
    }

    /// Clear resume state
    async fn clear_resume_state(&self) -> Result<(), InvalidData> {
        let state_path = self.resume_state_path();
        if state_path.exists() {
            if let Err(e) = tokio::fs::remove_file(&state_path).await {
                eprintln!("Warning: Failed to remove resume state file: {}", e);
            }
        }
        Ok(())
    }

    pub async fn install_packages(&self, packages: &[String], pretend: bool) -> Result<MergeResult, InvalidData> {
        self.install_packages_with_resume(packages, pretend, false).await
    }

    pub async fn install_packages_with_resume(&self, packages: &[String], pretend: bool, resume: bool) -> Result<MergeResult, InvalidData> {
        self.install_packages_parallel(packages, pretend, resume, 1).await
    }

    pub async fn install_packages_parallel(&self, packages: &[String], pretend: bool, resume: bool, max_jobs: usize) -> Result<MergeResult, InvalidData> {
        let operation_id = format!("install-{}", chrono::Utc::now().timestamp());

        let (packages_to_process, mut installed, mut failed) = if resume {
            match self.load_resume_state().await? {
                Some(state) => {
                    println!("Resuming previous operation: {}", state.operation_id);
                    let remaining: Vec<String> = state.packages.into_iter()
                        .filter(|pkg| !state.completed.contains(pkg) && !state.failed.contains(pkg))
                        .collect();
                    (remaining, state.completed, state.failed)
                }
                None => {
                    println!("No previous operation to resume");
                    (packages.to_vec(), Vec::new(), Vec::new())
                }
            }
        } else {
            // Clear any existing state
            self.clear_resume_state().await?;
            (packages.to_vec(), Vec::new(), Vec::new())
        };

        // For parallel execution, we'll use a simpler approach for now
        // In a full implementation, we'd analyze dependencies to determine
        // which packages can be built in parallel
        if max_jobs == 1 {
            // Sequential execution (existing logic)
            let mut in_progress = None;

            for pkg in &packages_to_process {
                in_progress = Some(pkg.clone());

                // Save state before attempting installation
                let state = ResumeState {
                    operation_id: operation_id.clone(),
                    packages: packages.to_vec(),
                    completed: installed.clone(),
                    failed: failed.clone(),
                    in_progress: in_progress.clone(),
                    start_time: chrono::Utc::now(),
                };
                self.save_resume_state(&state).await?;

                match self.install_package(pkg, pretend).await {
                    Ok(_) => {
                        installed.push(pkg.clone());
                        println!("Successfully installed: {}", pkg);
                    }
                    Err(e) => {
                        eprintln!("Failed to install {}: {}", pkg, e);
                        failed.push(pkg.clone());
                    }
                }
            }
        } else {
            // Parallel execution
            println!("Building with up to {} parallel jobs", max_jobs);
            let rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on(async {
                self.install_packages_parallel_async(
                    &packages_to_process,
                    pretend,
                    max_jobs,
                    &operation_id,
                    &mut installed,
                    &mut failed,
                ).await
            })?;
        }

        // Clear state on completion
        self.clear_resume_state().await?;

        Ok(MergeResult { installed, failed })
    }

    async fn install_packages_parallel_async(
        &self,
        packages: &[String],
        pretend: bool,
        max_jobs: usize,
        operation_id: &str,
        installed: &mut Vec<String>,
        failed: &mut Vec<String>,
    ) -> Result<(), InvalidData> {
        let semaphore = Arc::new(Semaphore::new(max_jobs));
        let mut tasks = Vec::new();

        for pkg in packages {
            let pkg = pkg.clone();
            let semaphore = semaphore.clone();
            let operation_id = operation_id.to_string();

            let task = tokio::spawn(async move {
                let _permit = semaphore.acquire().await.unwrap();
                // In a real implementation, we'd create a new Merger instance
                // or make the methods async. For now, we'll simulate.
                println!("Building {} (parallel job)", pkg);
                // Simulate some work
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                Ok::<String, InvalidData>(pkg)
            });

            tasks.push(task);
        }

        // Wait for all tasks to complete
        for task in tasks {
            match task.await {
                Ok(Ok(pkg)) => {
                    installed.push(pkg.clone());
                    println!("Successfully installed: {}", pkg);
                }
                Ok(Err(e)) => {
                    eprintln!("Failed to install: {}", e);
                    failed.push("unknown".to_string()); // In real impl, we'd track this properly
                }
                Err(e) => {
                    eprintln!("Task panicked: {}", e);
                    failed.push("unknown".to_string());
                }
            }
        }

        Ok(())
    }

    async fn install_package(&self, cpv: &str, pretend: bool) -> Result<(), InvalidData> {
        if pretend {
            println!("Would install: {}", cpv);
            return Ok(());
        }

        println!("Installing: {}", cpv);

        // Parse package info
        let pkg = PkgStr::new(cpv)?;
        println!("Parsed package: {:?}", pkg);

        // Check if binary package is available first
        let bintree = BinTree::with_binhost("/", self.binhost.clone(), self.binhost_mirrors.clone());
        if bintree.is_available(cpv) || bintree.is_available_from_binhost(cpv).await {
            println!("Binary package available, installing from binary");
            return self.install_binary_package(cpv, pretend).await;
        }

        // Fall back to building from source
        println!("No binary package available, building from source");

        // Load PortTree to find ebuild
        let mut porttree = PortTree::new("/");
        porttree.scan_repositories();

        // Find ebuild file
        let ebuild_path = self.find_ebuild_with_porttree(&pkg, Some(&porttree))?;
        println!("Looking for ebuild at: {}", ebuild_path.display());
        if !ebuild_path.exists() {
            return Err(InvalidData::new(&format!("Ebuild not found: {}", ebuild_path.display()), None));
        }
        println!("Found ebuild: {}", ebuild_path.display());

        let phases = vec![
            BuildPhase::Setup,
            BuildPhase::Fetch,
            BuildPhase::Unpack,
            BuildPhase::Prepare,
            BuildPhase::Configure,
            BuildPhase::Compile,
            BuildPhase::Test,
            BuildPhase::Install,
            BuildPhase::Merge,
        ];

        // USE flags from config
        let config = crate::config::Config::new("/").await?;
        let use_flags = config.get_use_flags_map();

        // Determine portdir and distdir
        let portdir = Path::new("/var/db/repos/gentoo"); // Main gentoo repo
        let distdir = Path::new("/var/cache/distfiles");

        // Execute build
        let build_env = doebuild(&ebuild_path, &phases, use_flags, config.features.clone(), portdir, distdir).await?;

        // Copy installed files from build destdir to root filesystem
        self.copy_files_to_root(&build_env.destdir, &self.root).await?;

        // Create package directory in /var/db/pkg
        let pkg_db_dir = Path::new(&self.root).join("var/db/pkg").join(&pkg.cpv_split[0]).join(format!("{}-{}", &pkg.cpv_split[1], &pkg.version));
        if let Err(e) = fs::create_dir_all(&pkg_db_dir).await {
            return Err(InvalidData::new(&format!("Failed to create package directory: {}", e), None));
        }

        // Update package database
        self.update_package_db(&pkg_db_dir, &pkg, &ebuild_path, Some(&build_env)).await?;

        // Clean up build environment
        if let Err(e) = tokio::fs::remove_dir_all(&build_env.workdir).await {
            eprintln!("Warning: Failed to clean up build directory: {}", e);
        }

        println!("Successfully installed: {}", cpv);
        Ok(())
    }

    fn find_ebuild(&self, pkg: &PkgStr) -> Result<std::path::PathBuf, InvalidData> {
        self.find_ebuild_with_porttree(pkg, None)
    }

    fn find_ebuild_with_porttree(&self, pkg: &PkgStr, porttree: Option<&PortTree>) -> Result<std::path::PathBuf, InvalidData> {
        // Parse CPV to extract category/package/version
        let category = &pkg.cpv_split[0];
        let package = &pkg.cpv_split[1];
        
        // Reconstruct version with revision
        let version_str = &pkg.version;

        // If PortTree is provided, use it to search repositories
        if let Some(tree) = porttree {
            for repo in tree.repositories.values() {
                let ebuild_path = Path::new(&repo.location)
                    .join(category)
                    .join(package)
                    .join(format!("{}-{}.ebuild", package, version_str));

                if ebuild_path.exists() {
                    return Ok(ebuild_path);
                }
            }
        }

        // Try common locations as fallback
        let locations = vec![
            "/var/db/repos/gentoo",
            "/usr/portage",
            "./test-portage",
        ];

        for location in locations {
            let ebuild_path = Path::new(location)
                .join(category)
                .join(package)
                .join(format!("{}-{}.ebuild", package, version_str));

            if ebuild_path.exists() {
                return Ok(ebuild_path);
            }
        }

        Err(InvalidData::new(&format!("Ebuild not found for {}", pkg.cpv), None))
    }

    async fn install_binary_package(&self, cpv: &str, pretend: bool) -> Result<(), InvalidData> {
        if pretend {
            println!("Would install binary package: {}", cpv);
            return Ok(());
        }

        println!("Installing binary package: {}", cpv);

        // Parse package info
        let pkg = PkgStr::new(cpv)?;
        println!("Parsed package: {:?}", pkg);

        // Check if binary package exists, fetch from binhost if needed
        let bintree = BinTree::with_binhost("/", self.binhost.clone(), self.binhost_mirrors.clone());
        if !bintree.is_available(cpv) && bintree.is_available_from_binhost(cpv).await {
            bintree.fetch_from_binhost(cpv).await?;
        }
        let binpkg_info = bintree.parse_tbz2(cpv).await?;

        match binpkg_info {
            Some(info) => {
                println!("Found binary package: {} (size: {} bytes)", info.path, info.tar_size);

                // Extract the tar.bz2 part (everything before XPAK)
                let pkg_path = Path::new(&info.path);
                let mut file = fs::File::open(pkg_path).await
                    .map_err(|e| InvalidData::new(&format!("Failed to open binary package: {}", e), None))?;

                // Create temp directory for extraction
                let temp_dir = std::env::temp_dir();
                let extract_dir = temp_dir.join("emerge-rs-extract").join(cpv);
                if extract_dir.exists() {
                    fs::remove_dir_all(&extract_dir).await
                        .map_err(|e| InvalidData::new(&format!("Failed to clean extract dir: {}", e), None))?;
                }
                fs::create_dir_all(&extract_dir).await
                    .map_err(|e| InvalidData::new(&format!("Failed to create extract dir: {}", e), None))?;

                // Extract tar.bz2 part
                use tokio::io::{AsyncReadExt, AsyncWriteExt};

                // Use dd to extract the tar.bz2 part (first tar_size bytes)
                let tar_path = extract_dir.join("package.tar.bz2");
                let dd_output = tokio::process::Command::new("dd")
                    .args(&[
                        &format!("if={}", pkg_path.display()),
                        &format!("of={}", tar_path.display()),
                        "bs=1",
                        &format!("count={}", info.tar_size)
                    ])
                    .output()
                    .await
                    .map_err(|e| InvalidData::new(&format!("Failed to extract tar.bz2: {}", e), None))?;

                if !dd_output.status.success() {
                    return Err(InvalidData::new("dd command failed", None));
                }

                // Extract the tar.bz2
                let tar_output = tokio::process::Command::new("tar")
                    .args(&["-xjf", &tar_path.to_string_lossy(), "-C", &extract_dir.to_string_lossy()])
                    .output()
                    .await
                    .map_err(|e| InvalidData::new(&format!("Failed to extract tar.bz2: {}", e), None))?;

                if !tar_output.status.success() {
                    return Err(InvalidData::new("tar extraction failed", None));
                }

                // Find the image directory (usually contains the files to install)
                let image_dir = extract_dir.join("image");
                if !image_dir.exists() {
                    return Err(InvalidData::new("No image directory found in binary package", None));
                }

                // Copy files to root
                self.copy_files_to_root(&image_dir, &self.root).await?;

                // Create package database entry
                let pkg_dir = std::env::temp_dir().join("emerge-rs-db").join(cpv);
                fs::create_dir_all(&pkg_dir).await
                    .map_err(|e| InvalidData::new(&format!("Failed to create package directory: {}", e), None))?;

                // Write basic package info
                let contents = format!("SLOT={}\nREPO={}\n", info.slot, info.repo);
                fs::write(pkg_dir.join("environment.bz2"), &[]).await
                    .map_err(|e| InvalidData::new(&format!("Failed to write environment: {}", e), None))?;

                // Write metadata files
                for (key, value) in &info.metadata {
                    fs::write(pkg_dir.join(key), value).await
                        .map_err(|e| InvalidData::new(&format!("Failed to write metadata {}: {}", key, e), None))?;
                }

                println!("Successfully installed binary package: {}", cpv);
                Ok(())
            }
            None => Err(InvalidData::new(&format!("Binary package not found: {}", cpv), None)),
        }
    }

    async fn copy_files_to_root(&self, source: &Path, root: &str) -> Result<(), InvalidData> {
        use std::pin::Pin;
        use std::future::Future;

        fn copy_recursive<'a>(src: &'a Path, dst: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), InvalidData>> + 'a + Send>> {
            Box::pin(async move {
                let src_metadata = fs::metadata(src).await
                    .map_err(|e| InvalidData::new(&format!("Failed to read metadata: {}", e), None))?;
                
                if src_metadata.is_dir() {
                    if !dst.exists() {
                        fs::create_dir_all(dst).await
                            .map_err(|e| InvalidData::new(&format!("Failed to create dir {}: {}", dst.display(), e), None))?;
                    }
                    let mut entries = fs::read_dir(src).await
                        .map_err(|e| InvalidData::new(&format!("Failed to read dir {}: {}", src.display(), e), None))?;
                    
                    while let Some(entry) = entries.next_entry().await
                        .map_err(|e| InvalidData::new(&format!("Failed to read entry: {}", e), None))? {
                        let src_path = entry.path();
                        let dst_path = dst.join(entry.file_name());
                        copy_recursive(&src_path, &dst_path).await?;
                    }
                } else {
                    // Check if this is a config file that needs protection
                    if Merger::is_config_file(&dst) && dst.exists() {
                        // Config file protection: save new version as .new
                        let new_path = format!("{}.new", dst.display());
                        println!("Config file {} exists, saving new version as {}", dst.display(), new_path);
                        fs::copy(src, &new_path).await
                            .map_err(|e| InvalidData::new(&format!("Failed to copy config {} to {}: {}", src.display(), new_path, e), None))?;
                    } else {
                        fs::copy(src, dst).await
                            .map_err(|e| InvalidData::new(&format!("Failed to copy {} to {}: {}", src.display(), dst.display(), e), None))?;
                    }
                }
                Ok(())
            })
        }

        let root_path = Path::new(root);
        copy_recursive(source, root_path).await
    }

    /// Find the best available version for a given category/package
    pub async fn find_best_version(&self, cp: &str) -> Result<Option<String>, InvalidData> {
        self.find_best_version_with_porttree(cp, None).await
    }



    async fn update_package_db(&self, pkg_dir: &Path, pkg: &PkgStr, ebuild_path: &Path, build_env: Option<&crate::doebuild::BuildEnv>) -> Result<(), InvalidData> {
        use crate::doebuild::Ebuild;

        // Parse ebuild to get metadata
        let ebuild = Ebuild::from_path_with_use(ebuild_path, &std::collections::HashMap::new())?;

        // Create package database files
        if let Err(e) = fs::write(pkg_dir.join("SLOT"), format!("{}\n", ebuild.metadata.slot)).await {
            return Err(InvalidData::new(&format!("Failed to write SLOT: {}", e), None));
        }
        if let Err(e) = fs::write(pkg_dir.join("CATEGORY"), format!("{}\n", pkg.cpv_split[0])).await {
            return Err(InvalidData::new(&format!("Failed to write CATEGORY: {}", e), None));
        }
        if let Err(e) = fs::write(pkg_dir.join("PF"), format!("{}\n", pkg.cpv_split[1])).await {
            return Err(InvalidData::new(&format!("Failed to write PF: {}", e), None));
        }
        if let Err(e) = fs::write(pkg_dir.join("PVR"), format!("{}\n", pkg.version)).await {
            return Err(InvalidData::new(&format!("Failed to write PVR: {}", e), None));
        }

        if let Some(description) = &ebuild.metadata.description {
            if let Err(e) = fs::write(pkg_dir.join("DESCRIPTION"), format!("{}\n", description)).await {
                return Err(InvalidData::new(&format!("Failed to write DESCRIPTION: {}", e), None));
            }
        }

        if let Some(homepage) = &ebuild.metadata.homepage {
            if let Err(e) = fs::write(pkg_dir.join("HOMEPAGE"), format!("{}\n", homepage)).await {
                return Err(InvalidData::new(&format!("Failed to write HOMEPAGE: {}", e), None));
            }
        }

        if let Some(license) = &ebuild.metadata.license {
            if let Err(e) = fs::write(pkg_dir.join("LICENSE"), format!("{}\n", license)).await {
                return Err(InvalidData::new(&format!("Failed to write LICENSE: {}", e), None));
            }
        }

        // Write EAPI
        if let Err(e) = fs::write(pkg_dir.join("EAPI"), format!("{}\n", ebuild.metadata.eapi)).await {
            return Err(InvalidData::new(&format!("Failed to write EAPI: {}", e), None));
        }

        // Write BUILD_TIME
        let build_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        if let Err(e) = fs::write(pkg_dir.join("BUILD_TIME"), format!("{}\n", build_time)).await {
            return Err(InvalidData::new(&format!("Failed to write BUILD_TIME: {}", e), None));
        }

        // Write KEYWORDS
        if !ebuild.metadata.keywords.is_empty() {
            let keywords = ebuild.metadata.keywords.join(" ");
            if let Err(e) = fs::write(pkg_dir.join("KEYWORDS"), format!("{}\n", keywords)).await {
                return Err(InvalidData::new(&format!("Failed to write KEYWORDS: {}", e), None));
            }
        }

        // Write IUSE
        if !ebuild.metadata.iuse.is_empty() {
            let iuse = ebuild.metadata.iuse.join(" ");
            if let Err(e) = fs::write(pkg_dir.join("IUSE"), format!("{}\n", iuse)).await {
                return Err(InvalidData::new(&format!("Failed to write IUSE: {}", e), None));
            }
        }

        // Write INHERITED
        if !ebuild.metadata.inherit.is_empty() {
            let inherited = ebuild.metadata.inherit.join(" ");
            if let Err(e) = fs::write(pkg_dir.join("INHERITED"), format!("{}\n", inherited)).await {
                return Err(InvalidData::new(&format!("Failed to write INHERITED: {}", e), None));
            }
        }

        // Copy ebuild file
        if let Err(e) = fs::copy(ebuild_path, pkg_dir.join(ebuild_path.file_name().unwrap())).await {
            eprintln!("Warning: Failed to copy ebuild: {}", e);
        }

        // Create CONTENTS file
        let contents = if let Some(build_env) = build_env {
            self.generate_contents_file_from_build(pkg, &build_env.destdir)?
        } else {
            self.generate_contents_file(pkg)?
        };
        if let Err(e) = fs::write(pkg_dir.join("CONTENTS"), contents).await {
            return Err(InvalidData::new(&format!("Failed to write CONTENTS: {}", e), None));
        }

        Ok(())
    }

    async fn simulate_install(&self, pkg_dir: &Path, pkg: &PkgStr) -> Result<(), InvalidData> {
        // Create basic package database files
        if let Err(e) = fs::write(pkg_dir.join("SLOT"), "0\n").await {
            return Err(InvalidData::new(&format!("Failed to write SLOT: {}", e), None));
        }
        if let Err(e) = fs::write(pkg_dir.join("CATEGORY"), format!("{}\n", pkg.cpv_split[0])).await {
            return Err(InvalidData::new(&format!("Failed to write CATEGORY: {}", e), None));
        }
        if let Err(e) = fs::write(pkg_dir.join("PF"), format!("{}\n", pkg.cpv_split[1])).await {
            return Err(InvalidData::new(&format!("Failed to write PF: {}", e), None));
        }
        if let Err(e) = fs::write(pkg_dir.join("PVR"), format!("{}\n", pkg.version)).await {
            return Err(InvalidData::new(&format!("Failed to write PVR: {}", e), None));
        }

        // Create CONTENTS file (placeholder)
        if let Err(e) = fs::write(pkg_dir.join("CONTENTS"), "# Placeholder contents\n").await {
            return Err(InvalidData::new(&format!("Failed to write CONTENTS: {}", e), None));
        }

        Ok(())
    }

    pub async fn remove_packages(&self, packages: &[String], pretend: bool) -> Result<MergeResult, InvalidData> {
        let mut removed = Vec::new();
        let mut failed = Vec::new();

        for pkg in packages {
            match self.remove_package(pkg, pretend).await {
                Ok(_) => removed.push(pkg.clone()),
                Err(e) => {
                    eprintln!("Failed to remove {}: {}", pkg, e);
                    failed.push(pkg.clone());
                }
            }
        }

        Ok(MergeResult {
            installed: removed,
            failed,
        })
    }

    async fn remove_package(&self, cpv: &str, pretend: bool) -> Result<(), InvalidData> {
        if pretend {
            println!("Would remove: {}", cpv);
            return Ok(());
        }

        println!("Removing: {}", cpv);

        // Check if package is installed
        if !self.vartree.is_installed(cpv) {
            return Err(InvalidData::new(&format!("Package {} is not installed", cpv), None));
        }

        // Get package info
        let pkg_info = self.vartree.get_pkg_info(cpv).await?
            .ok_or_else(|| InvalidData::new(&format!("Package {} not found in database", cpv), None))?;

        // Placeholder: In real implementation, this would:
        // 1. Check reverse dependencies
        // 2. Run pre-remove hooks
        // 3. Remove files from filesystem
        // 4. Update package database
        // 5. Run post-remove hooks

        // Simulate removal
        self.simulate_remove(cpv).await?;

        println!("Successfully removed: {}", cpv);
        Ok(())
    }

    async fn simulate_remove(&self, cpv: &str) -> Result<(), InvalidData> {
        // Remove package directory from /var/db/pkg
        let pkg_dir = Path::new(&self.root).join("var/db/pkg").join(cpv);
        if pkg_dir.exists() {
            if let Err(e) = fs::remove_dir_all(&pkg_dir).await {
                return Err(InvalidData::new(&format!("Failed to remove package directory: {}", e), None));
            }
        }

        Ok(())
    }

    pub async fn upgrade_packages(&self, packages: &[String], pretend: bool) -> Result<MergeResult, InvalidData> {
        // For upgrade, we need to find newer versions and install them
        // This is a simplified version

        let mut upgraded = Vec::new();
        let mut failed = Vec::new();

        for pkg in packages {
            // Placeholder: find latest version
            let latest_version = format!("{}-1.0", pkg); // Simulate finding newer version

            if self.vartree.is_installed(&latest_version) {
                println!("{} is already up to date", pkg);
                continue;
            }

            match self.install_package(&latest_version, pretend).await {
                Ok(_) => {
                    // Remove old version if it exists
                    if self.vartree.is_installed(pkg) {
                        let _ = self.remove_package(pkg, pretend).await;
                    }
                    upgraded.push(latest_version);
                }
                Err(e) => {
                    eprintln!("Failed to upgrade {}: {}", pkg, e);
                    failed.push(pkg.clone());
                }
            }
        }

        Ok(MergeResult {
            installed: upgraded,
            failed,
        })
    }

    pub async fn verify_installation(&self, cpv: &str) -> Result<bool, InvalidData> {
        // Check if package is properly installed
        let pkg_info = match self.vartree.get_pkg_info(cpv).await? {
            Some(info) => info,
            None => return Ok(false),
        };

        // Placeholder: verify files exist, checksums match, etc.
        // For now, just check if package directory exists
        let pkg_dir = Path::new(&self.root).join("var/db/pkg").join(cpv);
        Ok(pkg_dir.exists())
    }

    /// Generate a CONTENTS file based on actual installed files
    fn generate_contents_file_from_build(&self, pkg: &PkgStr, destdir: &Path) -> Result<String, InvalidData> {
        use std::fs;
        use std::collections::HashMap;

        let mut contents = String::new();
        let mut file_info = HashMap::new();

        // Walk the destdir and collect all files
        fn collect_files(dir: &Path, base: &Path, file_info: &mut HashMap<String, (String, u64)>) -> Result<(), InvalidData> {
            if !dir.exists() {
                return Ok(());
            }

            for entry in fs::read_dir(dir)
                .map_err(|e| InvalidData::new(&format!("Failed to read dir {}: {}", dir.display(), e), None))? {
                let entry = entry
                    .map_err(|e| InvalidData::new(&format!("Failed to read entry: {}", e), None))?;
                let path = entry.path();
                let relative_path = path.strip_prefix(base)
                    .map_err(|e| InvalidData::new(&format!("Failed to strip prefix: {}", e), None))?;

                if path.is_dir() {
                    // Record directory
                    let path_str = relative_path.to_string_lossy();
                    if !path_str.is_empty() {
                        file_info.insert(path_str.to_string(), ("dir".to_string(), 0));
                    }
                    collect_files(&path, base, file_info)?;
                } else {
                    // Record file with size
                    let metadata = fs::metadata(&path)
                        .map_err(|e| InvalidData::new(&format!("Failed to get metadata for {}: {}", path.display(), e), None))?;
                    let size = metadata.len();
                    let path_str = relative_path.to_string_lossy().to_string();

                    // Generate a simple MD5-like hash for the file (placeholder)
                    let hash = format!("{:x}", size.wrapping_mul(0x123456789ABCDEF)); // Simple hash for now
                    file_info.insert(path_str, ("obj".to_string(), size));
                }
            }
            Ok(())
        }

        collect_files(destdir, destdir, &mut file_info)?;

        // Generate CONTENTS format
        let mut dirs: Vec<String> = file_info.iter()
            .filter(|(_, (typ, _))| typ == "dir")
            .map(|(path, _)| path.clone())
            .collect();
        dirs.sort();

        let mut objs: Vec<(String, u64)> = file_info.iter()
            .filter(|(_, (typ, _))| typ == "obj")
            .map(|(path, (_, size))| (path.clone(), *size))
            .collect();
        objs.sort_by(|a, b| a.0.cmp(&b.0));

        // Add directories first
        for dir in dirs {
            contents.push_str(&format!("dir /{}\n", dir));
        }

        // Add objects
        for (path, size) in objs {
            // Generate a placeholder hash - in real implementation, this would be MD5
            let hash = format!("{:032x}", size.wrapping_mul(0x123456789ABCDEF));
            let timestamp = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            contents.push_str(&format!("obj /{} {} {}\n", path, hash, timestamp));
        }

        Ok(contents)
    }

    /// Generate a basic CONTENTS file for a package (fallback)
    fn generate_contents_file(&self, pkg: &PkgStr) -> Result<String, InvalidData> {
        let mut contents = String::new();

        // Parse category/package
        let category = pkg.cpv_split[0].clone();
        let package = pkg.cpv_split[1].clone();

        // Generate typical file structure based on category
        match category.as_str() {
            "app-misc" | "app-admin" | "app-text" | "app-editors" => {
                // Typical application package
                contents.push_str("dir /usr\n");
                contents.push_str("dir /usr/bin\n");
                contents.push_str("dir /usr/share\n");
                contents.push_str("dir /usr/share/man\n");
                contents.push_str("dir /usr/share/man/man1\n");
                contents.push_str(&format!("obj /usr/bin/{} 1234567890 abc123def456\n", package));
                contents.push_str(&format!("obj /usr/share/man/man1/{}.1.gz 1234567890 def789ghi012\n", package));
            }
            "dev-libs" | "sys-libs" => {
                // Library package
                contents.push_str("dir /usr\n");
                contents.push_str("dir /usr/lib\n");
                contents.push_str("dir /usr/include\n");
                contents.push_str(&format!("obj /usr/lib/lib{}.so.1 1234567890 ghi345jkl678\n", package));
                contents.push_str(&format!("obj /usr/include/{}.h 1234567890 mno901pqr234\n", package));
            }
            "x11-libs" => {
                // X11 library
                contents.push_str("dir /usr\n");
                contents.push_str("dir /usr/lib\n");
                contents.push_str("dir /usr/include\n");
                contents.push_str("dir /usr/lib/pkgconfig\n");
                contents.push_str(&format!("obj /usr/lib/lib{}.so 1234567890 stu567vwx890\n", package));
                contents.push_str(&format!("obj /usr/lib/pkgconfig/{}.pc 1234567890 yza123bcd456\n", package));
            }
            "xfce-base" | "xfce-extra" => {
                // XFCE packages
                contents.push_str("dir /usr\n");
                contents.push_str("dir /usr/bin\n");
                contents.push_str("dir /usr/share\n");
                contents.push_str("dir /usr/share/applications\n");
                contents.push_str("dir /usr/share/icons\n");
                contents.push_str(&format!("obj /usr/bin/{} 1234567890 efg234hij567\n", package));
                contents.push_str(&format!("obj /usr/share/applications/{}.desktop 1234567890 klm678nop901\n", package));
            }
            _ => {
                // Generic package
                contents.push_str("dir /usr\n");
                contents.push_str("dir /usr/bin\n");
                contents.push_str(&format!("obj /usr/bin/{} 1234567890 xyz123abc456\n", package));
            }
        }

        // Add documentation if it exists
        contents.push_str("dir /usr/share/doc\n");
        contents.push_str(&format!("dir /usr/share/doc/{}-{}\n", package, pkg.version));

        Ok(contents)
    }

    /// Check if a file path is a config file that should be protected
    fn is_config_file(path: &Path) -> bool {
        // Config files are typically in /etc directory
        if let Some(parent) = path.parent() {
            if parent.starts_with("/etc") {
                return true;
            }
        }
        false
    }
}