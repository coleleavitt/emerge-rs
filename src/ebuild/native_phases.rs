// native_phases.rs - Pure Rust phase executors (no bash)
//
// Implements all standard ebuild phases in native Rust

use std::path::Path;
use std::process::Command;
use std::os::unix::fs::PermissionsExt;
use crate::exception::InvalidData;
use super::environment::EbuildEnvironment;
use super::build_system::BuildSystem;
use super::archive::extract_archive;
use super::src_uri::{parse_src_uri, get_filename};
use super::helpers::{einfo, ewarn, ebegin, eend, eerror};
use super::eapi::Eapi;
use super::download::Downloader;
use std::collections::HashMap;

fn find_command_in_path(cmd: &str, env_vars: &HashMap<String, String>) -> Option<String> {
    let default_path = std::env::var("PATH").unwrap_or_else(|_| 
        "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string()
    );
    let path_var = env_vars.get("PATH")
        .map(|s| s.as_str())
        .unwrap_or(&default_path);
    
    for path_dir in path_var.split(':') {
        let cmd_path = Path::new(path_dir).join(cmd);
        if cmd_path.exists() && cmd_path.is_file() {
            if let Ok(metadata) = std::fs::metadata(&cmd_path) {
                if metadata.permissions().mode() & 0o111 != 0 {
                    return Some(cmd_path.to_string_lossy().to_string());
                }
            }
        }
    }
    
    None
}

fn check_command_exists(cmd: &str) -> bool {
    find_command_in_path(cmd, &HashMap::new()).is_some()
}

fn require_command(cmd: &str) -> Result<(), InvalidData> {
    if !check_command_exists(cmd) {
        let path_var = std::env::var("PATH").unwrap_or_else(|_| "not set".to_string());
        eerror(&format!("Required build tool '{}' not found in PATH", cmd));
        eerror(&format!("Current PATH: {}", path_var));
        eerror("Please install the required tool or ensure it's in your PATH");
        return Err(InvalidData::new(&format!("Build tool '{}' not found", cmd), None));
    }
    Ok(())
}

/// Native phase executor - runs phases without bash
pub struct NativePhaseExecutor {
    build_system: BuildSystem,
    eapi: Eapi,
}

impl NativePhaseExecutor {
    pub fn new(inherit: &[String], source_dir: &Path) -> Self {
        let build_system = BuildSystem::detect(inherit, source_dir);
        let eapi = Eapi::default();
        Self { build_system, eapi }
    }
    
    pub fn with_eapi(inherit: &[String], source_dir: &Path, eapi: Eapi) -> Self {
        let build_system = BuildSystem::detect(inherit, source_dir);
        Self { build_system, eapi }
    }
    
    pub fn fetch(&self, env: &EbuildEnvironment) -> Result<(), InvalidData> {
        einfo("Fetching source archives");
        
        let distdir = env.get("DISTDIR")
            .ok_or_else(|| InvalidData::new("DISTDIR not set", None))?;
        let src_uri = env.get("SRC_URI")
            .ok_or_else(|| InvalidData::new("SRC_URI not set", None))?;
        
        let mut vars = HashMap::new();
        for var in &["PN", "PV", "P", "PF", "MY_PN", "MY_PV", "MY_P", "CATEGORY", "CP", "CPV"] {
            if let Some(value) = env.get(*var) {
                vars.insert(var.to_string(), value.clone());
            }
        }
        
        let mut use_map = HashMap::new();
        for flag in &env.use_flags {
            use_map.insert(flag.clone(), true);
        }
        
        let uris = parse_src_uri(&src_uri, &vars, &use_map)
            .map_err(|e| InvalidData::new(&format!("Failed to parse SRC_URI: {}", e), None))?;
        
        let mut downloader = Downloader::new(distdir);
        
        let mirrors_file = Path::new("/var/db/repos/gentoo/profiles/thirdpartymirrors");
        if mirrors_file.exists() {
            downloader.load_thirdparty_mirrors(mirrors_file)?;
        }
        
        for uri in &uris {
            let filename = get_filename(uri);
            downloader.download(&uri.uri, &filename)?;
        }
        
        Ok(())
    }
    
    pub fn src_unpack(&self, env: &EbuildEnvironment) -> Result<(), InvalidData> {
        einfo("Unpacking sources");
        
        let distdir = env.get("DISTDIR")
            .ok_or_else(|| InvalidData::new("DISTDIR not set", None))?;
        let src_uri = env.get("SRC_URI")
            .ok_or_else(|| InvalidData::new("SRC_URI not set", None))?;
        
        let mut vars = HashMap::new();
        for var in &["PN", "PV", "P", "PF", "MY_PN", "MY_PV", "MY_P", "CATEGORY", "CP", "CPV"] {
            if let Some(value) = env.get(*var) {
                vars.insert(var.to_string(), value.clone());
            }
        }
        
        // Build USE flags map
        let mut use_map = HashMap::new();
        for flag in &env.use_flags {
            use_map.insert(flag.clone(), true);
        }
        
        // Parse SRC_URI
        let uris = parse_src_uri(&src_uri, &vars, &use_map)
            .map_err(|e| InvalidData::new(&format!("Failed to parse SRC_URI: {}", e), None))?;
        
        // Create work directory
        let workdir_str = env.get("WORKDIR").map(|s| s.as_str()).unwrap_or("/tmp");
        let workdir = Path::new(workdir_str);
        std::fs::create_dir_all(workdir)
            .map_err(|e| InvalidData::new(&format!("Failed to create WORKDIR: {}", e), None))?;
        
        // Extract each archive
        for uri in &uris {
            let filename = get_filename(uri);
            let archive_path = Path::new(distdir.as_str()).join(&filename);
            
            if !archive_path.exists() {
                return Err(InvalidData::new(&format!("Archive not found: {}", archive_path.display()), None));
            }
            
            einfo(&format!("Unpacking {}", filename));
            extract_archive(&archive_path, workdir)?;
        }
        
        Ok(())
    }
    
    /// Execute src_prepare phase natively
    pub fn src_prepare(&self, env: &EbuildEnvironment) -> Result<(), InvalidData> {
        einfo("Preparing sources");
        
        // Apply patches if any exist
        let filesdir = env.get("FILESDIR");
        if let Some(filesdir) = filesdir {
            let patches_dir = Path::new(filesdir.as_str()).join("patches");
            if patches_dir.exists() {
                self.apply_patches(&patches_dir, &env.sourcedir)?;
            }
        }
        
        Ok(())
    }
    
    /// Execute src_configure phase natively
    pub fn src_configure(&self, env: &EbuildEnvironment) -> Result<(), InvalidData> {
        einfo(&format!("Configuring with {:?}", self.build_system));
        
        let args = self.build_system.default_configure_args();
        
        if let Some((cmd, cmd_args)) = self.build_system.configure_command(&args) {
            require_command(cmd)?;
            
            ebegin(&format!("Running {}", cmd));
            
            let cmd_path = if cmd.starts_with('/') {
                cmd.to_string()
            } else {
                find_command_in_path(cmd, &env.vars).unwrap_or_else(|| cmd.to_string())
            };
            
            let mut command = Command::new(&cmd_path);
            command.env_clear();
            for (key, value) in &env.vars {
                command.env(key, value);
            }
            
            let status = command
                .args(&cmd_args)
                .current_dir(&env.sourcedir)
                .status()
                .map_err(|e| InvalidData::new(&format!("Failed to run {}: {}", cmd, e), None))?;
            
            eend(if status.success() { 0 } else { 1 }, None);
            
            if !status.success() {
                return Err(InvalidData::new(&format!("{} failed", cmd), None));
            }
        }
        
        Ok(())
    }
    
    /// Execute src_compile phase natively
    pub fn src_compile(&self, env: &EbuildEnvironment) -> Result<(), InvalidData> {
        // Check if this is a binary package (package name ends with -bin)
        if let Some(pn) = env.get("PN") {
            if pn.ends_with("-bin") {
                einfo("Binary package, skipping compilation");
                return Ok(());
            }
        }
        
        einfo(&format!("Compiling with {:?}", self.build_system));
        
        let (cmd, args) = self.build_system.build_command();
        
        require_command(cmd)?;
        
        ebegin(&format!("Running {}", cmd));
        
        let cmd_path = if cmd.starts_with('/') {
            cmd.to_string()
        } else {
            find_command_in_path(cmd, &env.vars).unwrap_or_else(|| cmd.to_string())
        };
        
        let mut command = Command::new(&cmd_path);
        command.env_clear();
        for (key, value) in &env.vars {
            command.env(key, value);
        }
        
        let output = command
            .args(&args)
            .current_dir(&env.sourcedir)
            .output()
            .map_err(|e| InvalidData::new(&format!("Failed to run {}: {}", cmd, e), None))?;
        
        // Print stdout/stderr
        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }
        
        eend(if output.status.success() { 0 } else { 1 }, None);
        
        if !output.status.success() {
            return Err(InvalidData::new(&format!("{} failed", cmd), None));
        }
        
        Ok(())
    }
    
    /// Execute src_test phase natively
    pub fn src_test(&self, env: &EbuildEnvironment) -> Result<(), InvalidData> {
        einfo("Running tests");
        
        let (cmd, args) = match &self.build_system {
            BuildSystem::Makefile => ("make", vec!["check"]),
            BuildSystem::CMake => ("make", vec!["test"]),
            BuildSystem::Meson => ("meson", vec!["test"]),
            BuildSystem::Cargo => ("cargo", vec!["test"]),
            BuildSystem::Autotools => ("make", vec!["check"]),
            BuildSystem::Python => {
                einfo("No tests defined for Python build system");
                return Ok(());
            }
            BuildSystem::Go => ("go", vec!["test", "./..."]),
            BuildSystem::Custom => {
                einfo("No tests defined for this build system");
                return Ok(());
            }
        };
        
        require_command(cmd)?;
        
        ebegin(&format!("Running {}", cmd));
        
        let cmd_path = if cmd.starts_with('/') {
            cmd.to_string()
        } else {
            find_command_in_path(cmd, &env.vars).unwrap_or_else(|| cmd.to_string())
        };
        
        let mut command = Command::new(&cmd_path);
        command.env_clear();
        for (key, value) in &env.vars {
            command.env(key, value);
        }
        
        let output = command
            .args(&args)
            .current_dir(&env.sourcedir)
            .output()
            .map_err(|e| InvalidData::new(&format!("Failed to run {}: {}", cmd, e), None))?;
        
        // Print stdout/stderr
        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }
        
        eend(if output.status.success() { 0 } else { 1 }, None);
        
        if !output.status.success() {
            ewarn("Tests failed (non-fatal)");
        }
        
        Ok(())
    }
    
    pub fn src_install(&self, env: &EbuildEnvironment) -> Result<(), InvalidData> {
        einfo("Installing");
        
        let destdir = env.destdir.to_string_lossy().to_string();
        let (cmd, args) = self.build_system.install_command(&destdir);
        
        require_command(cmd)?;
        
        ebegin(&format!("Running {} install", cmd));
        
        let cmd_path = if cmd.starts_with('/') {
            cmd.to_string()
        } else {
            find_command_in_path(cmd, &env.vars).unwrap_or_else(|| cmd.to_string())
        };
        
        let mut command = Command::new(&cmd_path);
        command.env_clear();
        for (key, value) in &env.vars {
            command.env(key, value);
        }
        command.env("DESTDIR", &destdir);
        
        let output = command
            .args(&args)
            .current_dir(&env.sourcedir)
            .output()
            .map_err(|e| InvalidData::new(&format!("Failed to run {} install: {}", cmd, e), None))?;
        
        // Print stdout/stderr
        if !output.stdout.is_empty() {
            print!("{}", String::from_utf8_lossy(&output.stdout));
        }
        if !output.stderr.is_empty() {
            eprint!("{}", String::from_utf8_lossy(&output.stderr));
        }
        
        eend(if output.status.success() { 0 } else { 1 }, None);
        
        if !output.status.success() {
            return Err(InvalidData::new(&format!("{} install failed", cmd), None));
        }
        
        Ok(())
    }
    
    pub fn merge(&self, env: &EbuildEnvironment, root: &str) -> Result<(), InvalidData> {
        use std::fs;
        
        
        einfo(&format!("Merging to {}", root));
        
        let image_dir = &env.destdir;
        if !image_dir.exists() {
            return Err(InvalidData::new("Image directory does not exist", None));
        }
        
        fn copy_recursively(src: &Path, dst: &Path, root: &str) -> Result<(), InvalidData> {
            if src.is_dir() {
                fs::create_dir_all(dst)
                    .map_err(|e| InvalidData::new(&format!("Failed to create directory {}: {}", dst.display(), e), None))?;
                
                for entry in fs::read_dir(src)
                    .map_err(|e| InvalidData::new(&format!("Failed to read directory {}: {}", src.display(), e), None))? {
                    let entry = entry
                        .map_err(|e| InvalidData::new(&format!("Failed to read entry: {}", e), None))?;
                    let src_path = entry.path();
                    let dst_path = dst.join(entry.file_name());
                    copy_recursively(&src_path, &dst_path, root)?;
                }
            } else {
                fs::copy(&src, &dst)
                    .map_err(|e| InvalidData::new(&format!("Failed to copy {} to {}: {}", src.display(), dst.display(), e), None))?;
                
                if let Ok(metadata) = fs::metadata(&src) {
                    fs::set_permissions(&dst, metadata.permissions())
                        .map_err(|e| InvalidData::new(&format!("Failed to set permissions on {}: {}", dst.display(), e), None))?;
                }
            }
            Ok(())
        }
        
        for entry in fs::read_dir(image_dir)
            .map_err(|e| InvalidData::new(&format!("Failed to read image directory: {}", e), None))? {
            let entry = entry
                .map_err(|e| InvalidData::new(&format!("Failed to read entry: {}", e), None))?;
            let src_path = entry.path();
            let relative_path = src_path.strip_prefix(image_dir)
                .map_err(|e| InvalidData::new(&format!("Failed to strip prefix: {}", e), None))?;
            let dst_path = Path::new(root).join(relative_path);
            
            einfo(&format!("Installing {}", relative_path.display()));
            copy_recursively(&src_path, &dst_path, root)?;
        }
        
        Ok(())
    }
    
    /// Apply patches from directory
    fn apply_patches(&self, patches_dir: &Path, source_dir: &Path) -> Result<(), InvalidData> {
        let entries = std::fs::read_dir(patches_dir)
            .map_err(|e| InvalidData::new(&format!("Failed to read patches dir: {}", e), None))?;
        
        let mut patches = Vec::new();
        
        for entry in entries {
            let entry = entry.map_err(|e| InvalidData::new(&format!("Failed to read patch entry: {}", e), None))?;
            let path = entry.path();
            
            if path.extension().map(|e| e == "patch" || e == "diff").unwrap_or(false) {
                patches.push(path);
            }
        }
        
        patches.sort();
        
        for patch in patches {
            einfo(&format!("Applying {}", patch.display()));
            
            let status = Command::new("patch")
                .arg("-p1")
                .arg("-i")
                .arg(&patch)
                .current_dir(source_dir)
                .status()
                .map_err(|e| InvalidData::new(&format!("Failed to apply patch: {}", e), None))?;
            
            if !status.success() {
                return Err(InvalidData::new(&format!("Patch {} failed", patch.display()), None));
            }
        }
        
        Ok(())
    }
}
