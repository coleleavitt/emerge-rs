// eclass.rs - Eclass support and common eclass functions
use std::collections::HashMap;
use crate::exception::InvalidData;
use super::environment::EbuildEnvironment;
use super::helpers::{einfo, default_src_prepare};

/// Eclass manager
pub struct EclassManager {
    /// Loaded eclasses
    eclasses: HashMap<String, Eclass>,
}

/// Represents an eclass
pub struct Eclass {
    pub name: String,
    pub functions: HashMap<String, String>,
}

impl EclassManager {
    pub fn new() -> Self {
        let mut manager = EclassManager {
            eclasses: HashMap::new(),
        };
        
        // Register built-in eclasses
        manager.register_toolchain_funcs();
        manager.register_cmake();
        manager.register_meson();
        manager.register_savedconfig();
        manager.register_unpacker();
        manager.register_xdg();
        manager.register_systemd();
        manager.register_bash_completion_r1();
        manager.register_chromium_2();
        
        manager
    }
    
    /// Register toolchain-funcs eclass helpers
    fn register_toolchain_funcs(&mut self) {
        let mut eclass = Eclass {
            name: "toolchain-funcs".to_string(),
            functions: HashMap::new(),
        };
        
        // tc-export is a common function
        eclass.functions.insert("tc-export".to_string(), 
            "# Export toolchain variables".to_string());
        
        self.eclasses.insert("toolchain-funcs".to_string(), eclass);
    }
    
    /// Register cmake eclass helpers
    fn register_cmake(&mut self) {
        let mut eclass = Eclass {
            name: "cmake".to_string(),
            functions: HashMap::new(),
        };
        
        eclass.functions.insert("cmake_src_prepare".to_string(),
            "# CMake source prepare".to_string());
        eclass.functions.insert("cmake_src_configure".to_string(),
            "# CMake configure".to_string());
        eclass.functions.insert("cmake_src_compile".to_string(),
            "# CMake compile".to_string());
        eclass.functions.insert("cmake_src_install".to_string(),
            "# CMake install".to_string());
        
        self.eclasses.insert("cmake".to_string(), eclass);
    }
    
    /// Register meson eclass helpers
    fn register_meson(&mut self) {
        let mut eclass = Eclass {
            name: "meson".to_string(),
            functions: HashMap::new(),
        };
        
        eclass.functions.insert("meson_src_configure".to_string(),
            "# Meson configure".to_string());
        eclass.functions.insert("meson_use".to_string(),
            "# Meson USE flag helper".to_string());
        eclass.functions.insert("meson_feature".to_string(),
            "# Meson feature helper".to_string());
        
        self.eclasses.insert("meson".to_string(), eclass);
    }

    /// Register savedconfig eclass helpers
    fn register_savedconfig(&mut self) {
        let mut eclass = Eclass {
            name: "savedconfig".to_string(),
            functions: HashMap::new(),
        };

        eclass.functions.insert("restore_config".to_string(),
            "# Restore saved configuration".to_string());
        eclass.functions.insert("save_config".to_string(),
            "# Save configuration".to_string());

        self.eclasses.insert("savedconfig".to_string(), eclass);
    }

    /// Register unpacker eclass helpers
    fn register_unpacker(&mut self) {
        let mut eclass = Eclass {
            name: "unpacker".to_string(),
            functions: HashMap::new(),
        };

        eclass.functions.insert("unpacker_src_unpack".to_string(),
            "# Unpacker source unpack".to_string());
        eclass.functions.insert("unpacker_src_prepare".to_string(),
            "# Unpacker source prepare".to_string());

        self.eclasses.insert("unpacker".to_string(), eclass);
    }

    /// Register xdg eclass helpers
    fn register_xdg(&mut self) {
        let mut eclass = Eclass {
            name: "xdg".to_string(),
            functions: HashMap::new(),
        };

        eclass.functions.insert("xdg_src_prepare".to_string(),
            "# XDG source prepare".to_string());
        eclass.functions.insert("xdg_src_install".to_string(),
            "# XDG install".to_string());
        eclass.functions.insert("xdg_pkg_preinst".to_string(),
            "# XDG preinst".to_string());
        eclass.functions.insert("xdg_pkg_postinst".to_string(),
            "# XDG postinst".to_string());
        eclass.functions.insert("xdg_pkg_postrm".to_string(),
            "# XDG postrm".to_string());

        self.eclasses.insert("xdg".to_string(), eclass);
    }

    /// Register systemd eclass helpers
    fn register_systemd(&mut self) {
        let mut eclass = Eclass {
            name: "systemd".to_string(),
            functions: HashMap::new(),
        };

        eclass.functions.insert("systemd_get_systemunitdir".to_string(),
            "# Get systemd system unit directory".to_string());
        eclass.functions.insert("systemd_get_userunitdir".to_string(),
            "# Get systemd user unit directory".to_string());
        eclass.functions.insert("systemd_dounit".to_string(),
            "# Install systemd unit files".to_string());
        eclass.functions.insert("systemd_enable_service".to_string(),
            "# Enable systemd service".to_string());
        eclass.functions.insert("systemd_update_catalog".to_string(),
            "# Update systemd catalog".to_string());

        self.eclasses.insert("systemd".to_string(), eclass);
    }

    /// Register bash-completion-r1 eclass helpers
    fn register_bash_completion_r1(&mut self) {
        let mut eclass = Eclass {
            name: "bash-completion-r1".to_string(),
            functions: HashMap::new(),
        };

        eclass.functions.insert("bashcomp_alias".to_string(),
            "# Create bash completion alias".to_string());
        eclass.functions.insert("dobashcomp".to_string(),
            "# Install bash completion files".to_string());
        eclass.functions.insert("newbashcomp".to_string(),
            "# Install bash completion file with new name".to_string());

        self.eclasses.insert("bash-completion-r1".to_string(), eclass);
    }

    /// Register chromium-2 eclass helpers
    fn register_chromium_2(&mut self) {
        let mut eclass = Eclass {
            name: "chromium-2".to_string(),
            functions: HashMap::new(),
        };

        eclass.functions.insert("chromium_src_unpack".to_string(),
            "# Chromium source unpack - handles deb files".to_string());

        self.eclasses.insert("chromium-2".to_string(), eclass);
    }

    /// Check if eclass is loaded
    pub fn has_eclass(&self, name: &str) -> bool {
        self.eclasses.contains_key(name)
    }
    
    /// Get eclass function
    pub fn get_function(&self, eclass: &str, func: &str) -> Option<&String> {
        self.eclasses.get(eclass)
            .and_then(|e| e.functions.get(func))
    }
}

/// Toolchain functions
pub mod tc {
    use super::*;
    use std::process::Command;
    
    /// Export toolchain variables
    pub fn export(env: &mut EbuildEnvironment, vars: &[&str]) -> Result<(), InvalidData> {
        for var in vars {
            match *var {
                "CC" => env.set("CC".to_string(), std::env::var("CC").unwrap_or_else(|_| "gcc".to_string())),
                "CXX" => env.set("CXX".to_string(), std::env::var("CXX").unwrap_or_else(|_| "g++".to_string())),
                "AR" => env.set("AR".to_string(), std::env::var("AR").unwrap_or_else(|_| "ar".to_string())),
                "AS" => env.set("AS".to_string(), std::env::var("AS").unwrap_or_else(|_| "as".to_string())),
                "LD" => env.set("LD".to_string(), std::env::var("LD").unwrap_or_else(|_| "ld".to_string())),
                "NM" => env.set("NM".to_string(), std::env::var("NM").unwrap_or_else(|_| "nm".to_string())),
                "RANLIB" => env.set("RANLIB".to_string(), std::env::var("RANLIB").unwrap_or_else(|_| "ranlib".to_string())),
                "STRIP" => env.set("STRIP".to_string(), std::env::var("STRIP").unwrap_or_else(|_| "strip".to_string())),
                "PKG_CONFIG" => env.set("PKG_CONFIG".to_string(), std::env::var("PKG_CONFIG").unwrap_or_else(|_| "pkg-config".to_string())),
                _ => return Err(InvalidData::new(&format!("tc-export: invalid export variable '{}'", var), None)),
            }
        }
        Ok(())
    }
    
    /// Get C++ standard library
    pub fn get_cxx_stdlib() -> String {
        let output = Command::new("sh")
            .arg("-c")
            .arg("${CXX:-g++} -print-file-name=libstdc++.so")
            .output();
        
        match output {
            Ok(out) if out.status.success() => {
                let path = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if path.contains("libstdc++") {
                    "libstdc++".to_string()
                } else if path.contains("libc++") {
                    "libc++".to_string()
                } else {
                    "libstdc++".to_string()
                }
            }
            _ => "libstdc++".to_string(),
        }
    }
    
    /// Check if LTO is enabled
    pub fn is_lto() -> bool {
        std::env::var("CFLAGS")
            .or_else(|_| std::env::var("CXXFLAGS"))
            .map(|flags| flags.contains("-flto"))
            .unwrap_or(false)
    }
    
    /// Get compiler
    pub fn get_cc() -> String {
        std::env::var("CC").unwrap_or_else(|_| "gcc".to_string())
    }
    
    /// Get C++ compiler
    pub fn get_cxx() -> String {
        std::env::var("CXX").unwrap_or_else(|_| "g++".to_string())
    }
    
    /// Get archiver
    pub fn get_ar() -> String {
        std::env::var("AR").unwrap_or_else(|_| "ar".to_string())
    }
    
    /// Get pkg-config
    pub fn get_pkg_config() -> String {
        std::env::var("PKG_CONFIG").unwrap_or_else(|_| "pkg-config".to_string())
    }
}

/// CMake eclass functions  
pub mod cmake {
    use super::*;
    use std::process::Command;
    
    pub fn src_prepare(env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        // Run default prepare
        Ok(())
    }
    
    pub fn src_configure(env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        let build_dir = env.builddir.clone();
        std::fs::create_dir_all(&build_dir)
            .map_err(|e| InvalidData::new(&format!("Failed to create build dir: {}", e), None))?;
        
        let output = Command::new("cmake")
            .arg(&env.sourcedir)
            .arg(format!("-DCMAKE_INSTALL_PREFIX={}/usr", env.destdir.display()))
            .current_dir(&build_dir)
            .output()
            .map_err(|e| InvalidData::new(&format!("CMake failed: {}", e), None))?;
        
        if !output.status.success() {
            return Err(InvalidData::new("CMake configuration failed", None));
        }
        
        Ok(())
    }
    
    pub fn src_compile(env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        let output = Command::new("cmake")
            .args(&["--build", "."])
            .current_dir(&env.builddir)
            .output()
            .map_err(|e| InvalidData::new(&format!("CMake build failed: {}", e), None))?;
        
        if !output.status.success() {
            return Err(InvalidData::new("CMake build failed", None));
        }
        
        Ok(())
    }
    
    pub fn src_install(env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        let output = Command::new("cmake")
            .args(&["--install", "."])
            .current_dir(&env.builddir)
            .output()
            .map_err(|e| InvalidData::new(&format!("CMake install failed: {}", e), None))?;
        
        if !output.status.success() {
            return Err(InvalidData::new("CMake install failed", None));
        }
        
        Ok(())
    }
}

/// Meson eclass functions
pub mod meson {
    use super::*;
    use std::process::Command;
    
    pub fn src_configure(env: &mut EbuildEnvironment, mycmakeargs: &[String]) -> Result<(), InvalidData> {
        let build_dir = env.builddir.clone();
        
        let mut cmd = Command::new("meson");
        cmd.arg("setup");
        cmd.arg(&build_dir);
        cmd.arg(&env.sourcedir);
        cmd.arg(format!("--prefix={}/usr", env.destdir.display()));
        
        for arg in mycmakeargs {
            cmd.arg(arg);
        }
        
        let output = cmd.output()
            .map_err(|e| InvalidData::new(&format!("Meson failed: {}", e), None))?;
        
        if !output.status.success() {
            return Err(InvalidData::new("Meson configuration failed", None));
        }
        
        Ok(())
    }
    
    /// Helper for meson_use: -Doption=enabled/disabled based on USE flag
    pub fn use_flag(env: &EbuildEnvironment, flag: &str, option: &str) -> String {
        if env.use_flag_enabled(flag) {
            format!("-D{}=enabled", option)
        } else {
            format!("-D{}=disabled", option)
        }
    }
    
    /// Alias for use_flag - meson_feature is same as meson_use
    pub fn feature(env: &EbuildEnvironment, flag: &str, option: &str) -> String {
        use_flag(env, flag, option)
    }
    
    /// Simple wrapper: returns "enabled" or "disabled" string
    pub fn enabled_disabled(env: &EbuildEnvironment, flag: &str) -> &'static str {
        if env.use_flag_enabled(flag) {
            "enabled"
        } else {
            "disabled"
        }
    }
    
    /// Simple wrapper: returns "true" or "false" string
    pub fn true_false(env: &EbuildEnvironment, flag: &str) -> &'static str {
        if env.use_flag_enabled(flag) {
            "true"
        } else {
            "false"
        }
    }
}

/// Savedconfig eclass functions
pub mod savedconfig {
    use super::*;
    use std::fs;
    

    /// Restore saved configuration from a config file
    pub fn restore_config(env: &mut EbuildEnvironment, config_file: &str) -> Result<(), InvalidData> {
        let config_path = env.sourcedir.join(config_file);
        if !config_path.exists() {
            // Config file doesn't exist, nothing to restore
            return Ok(());
        }

        // Read the config file
        let content = fs::read_to_string(&config_path)
            .map_err(|e| InvalidData::new(&format!("Failed to read config file {}: {}", config_file, e), None))?;

        // Parse the config file (simple format: one file per line, # for comments)
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            // For now, just ensure the file exists or create it
            // In a full implementation, this would restore specific configurations
            let file_path = env.sourcedir.join(line);
            if !file_path.exists() {
                // Create empty file if it doesn't exist
                if let Some(parent) = file_path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|e| InvalidData::new(&format!("Failed to create directory for {}: {}", line, e), None))?;
                }
                fs::File::create(&file_path)
                    .map_err(|e| InvalidData::new(&format!("Failed to create config file {}: {}", line, e), None))?;
            }
        }

        Ok(())
    }

    /// Save configuration to a config file
    pub fn save_config(env: &EbuildEnvironment, config_file: &str) -> Result<(), InvalidData> {
        let config_path = env.sourcedir.join(config_file);

        // For now, create a basic config file
        // In a full implementation, this would save current configuration state
        let content = "# Saved configuration\n# This file contains the list of configuration files\n";

        fs::write(&config_path, content)
            .map_err(|e| InvalidData::new(&format!("Failed to save config file {}: {}", config_file, e), None))?;

        Ok(())
    }
}

/// Unpacker eclass functions
pub mod unpacker {
    use super::*;
    use std::fs;
    use std::path::Path;
    use std::process::Command;
    use crate::ebuild::helpers;

    pub fn src_unpack(env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        // Get SRC_URI and A (archive files) from environment
        let distdir = env.get("DISTDIR").map(|s| s.clone()).unwrap_or_else(|| "/var/cache/distfiles".to_string());
        let a = env.get("A").map(|s| s.clone()).unwrap_or_default();

        if a.is_empty() {
            return Ok(());
        }

        // Create work directory
        fs::create_dir_all(&env.sourcedir)
            .map_err(|e| InvalidData::new(&format!("Failed to create source directory: {}", e), None))?;

        // Unpack each archive with additional formats supported by unpacker
        for archive in a.split_whitespace() {
            let archive_path = Path::new(&distdir).join(archive);

            if !archive_path.exists() {
                return Err(InvalidData::new(&format!("Archive {} not found", archive), None));
            }

            einfo(&format!("Unpacking {}", archive));

            // Try various unpacking methods
            unpack_archive(&archive_path, &env.sourcedir)?;
        }

        Ok(())
    }

    pub fn src_prepare(env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        // Apply patches and prepare source
        default_src_prepare(env)
    }

    fn unpack_archive(archive: &Path, dest: &Path) -> Result<(), InvalidData> {
        let filename = archive.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");

        // Try different unpacking methods based on extension
        if filename.ends_with(".tar.gz") || filename.ends_with(".tgz") ||
           filename.ends_with(".tar.bz2") || filename.ends_with(".tbz2") ||
           filename.ends_with(".tar.xz") || filename.ends_with(".txz") ||
           filename.ends_with(".tar.zst") || filename.ends_with(".tar") {
            helpers::unpack_tar(archive, dest, None)
        } else if filename.ends_with(".zip") {
            helpers::unpack_zip(archive, dest)
        } else if filename.ends_with(".rar") {
            unpack_rar(archive, dest)
        } else if filename.ends_with(".7z") {
            unpack_7z(archive, dest)
        } else {
            // Try generic unpacking
            Err(InvalidData::new(&format!("Unknown archive format: {}", filename), None))
        }
    }

    fn unpack_rar(archive: &Path, dest: &Path) -> Result<(), InvalidData> {
        let output = Command::new("unrar")
            .arg("x")
            .arg("-o+")
            .arg(archive)
            .arg(dest)
            .output()
            .map_err(|e| InvalidData::new(&format!("Failed to run unrar: {}", e), None))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(InvalidData::new("unrar failed", None))
        }
    }

    fn unpack_7z(archive: &Path, dest: &Path) -> Result<(), InvalidData> {
        let output = Command::new("7z")
            .arg("x")
            .arg(archive)
            .arg(&format!("-o{}", dest.display()))
            .output()
            .map_err(|e| InvalidData::new(&format!("Failed to run 7z: {}", e), None))?;

        if output.status.success() {
            Ok(())
        } else {
            Err(InvalidData::new("7z failed", None))
        }
    }
}

/// XDG eclass functions
pub mod xdg {
    use super::*;
    
    

    pub fn src_prepare(env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        // Ensure XDG directories exist
        Ok(())
    }

    pub fn src_install(env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        // Install XDG compliant files
        Ok(())
    }

    pub fn pkg_preinst(env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        // Pre-install XDG setup
        Ok(())
    }

    pub fn pkg_postinst(env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        // Update XDG icon cache, desktop database, etc.
        update_icon_cache()?;
        update_desktop_database()?;
        update_mime_database()?;
        Ok(())
    }

    pub fn pkg_postrm(env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
        // Clean up XDG caches after removal
        update_icon_cache()?;
        update_desktop_database()?;
        update_mime_database()?;
        Ok(())
    }

    fn update_icon_cache() -> Result<(), InvalidData> {
        let output = std::process::Command::new("gtk-update-icon-cache")
            .arg("-f")
            .arg("-t")
            .arg("/usr/share/icons/hicolor")
            .output();

        match output {
            Ok(_) => Ok(()),
            Err(_) => Ok(()), // Ignore errors if command not available
        }
    }

    fn update_desktop_database() -> Result<(), InvalidData> {
        let output = std::process::Command::new("update-desktop-database")
            .arg("/usr/share/applications")
            .output();

        match output {
            Ok(_) => Ok(()),
            Err(_) => Ok(()), // Ignore errors if command not available
        }
    }

    fn update_mime_database() -> Result<(), InvalidData> {
        let output = std::process::Command::new("update-mime-database")
            .arg("/usr/share/mime")
            .output();

        match output {
            Ok(_) => Ok(()),
            Err(_) => Ok(()), // Ignore errors if command not available
        }
    }
}

/// Systemd eclass functions
pub mod systemd {
    use super::*;
    use std::fs;
    use std::path::Path;

    pub fn get_systemunitdir() -> String {
        "/usr/lib/systemd/system".to_string()
    }

    pub fn get_userunitdir() -> String {
        "/usr/lib/systemd/user".to_string()
    }

    pub fn dounit(env: &EbuildEnvironment, units: &[&str]) -> Result<(), InvalidData> {
        let unitdir = get_systemunitdir();
        let dest_dir = env.destdir.join(unitdir.trim_start_matches('/'));

        fs::create_dir_all(&dest_dir)
            .map_err(|e| InvalidData::new(&format!("Failed to create systemd unit dir: {}", e), None))?;

        for unit in units {
            let src = Path::new(unit);
            if !src.exists() {
                return Err(InvalidData::new(&format!("Systemd unit {} not found", unit), None));
            }

            let filename = src.file_name()
                .ok_or_else(|| InvalidData::new(&format!("Invalid unit filename: {}", unit), None))?;
            let dest = dest_dir.join(filename);

            fs::copy(src, &dest)
                .map_err(|e| InvalidData::new(&format!("Failed to copy unit {}: {}", unit, e), None))?;
        }

        Ok(())
    }

    pub fn enable_service(env: &EbuildEnvironment, service: &str) -> Result<(), InvalidData> {
        // This would typically use systemctl, but for ebuild installation
        // we just ensure the service file is installed
        // Actual enabling happens at runtime
        Ok(())
    }

    pub fn update_catalog() -> Result<(), InvalidData> {
        let output = std::process::Command::new("systemd-hwdb")
            .arg("update")
            .output();

        match output {
            Ok(_) => Ok(()),
            Err(_) => Ok(()), // Ignore if not available
        }
    }
}

/// Bash completion eclass functions
pub mod bash_completion_r1 {
    use super::*;
    use std::fs;
    use std::path::Path;

    pub fn alias(name: &str, target: &str) -> String {
        format!("alias {}='{}'", name, target)
    }

    pub fn dobashcomp(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
        let comp_dir = env.destdir.join("usr/share/bash-completion/completions");
        fs::create_dir_all(&comp_dir)
            .map_err(|e| InvalidData::new(&format!("Failed to create bash completion dir: {}", e), None))?;

        for file in files {
            let src = Path::new(file);
            if !src.exists() {
                return Err(InvalidData::new(&format!("Bash completion file {} not found", file), None));
            }

            let filename = src.file_name()
                .ok_or_else(|| InvalidData::new(&format!("Invalid completion filename: {}", file), None))?;
            let dest = comp_dir.join(filename);

            fs::copy(src, &dest)
                .map_err(|e| InvalidData::new(&format!("Failed to copy completion {}: {}", file, e), None))?;
        }

        Ok(())
    }

    pub fn newbashcomp(env: &EbuildEnvironment, src: &str, dest_name: &str) -> Result<(), InvalidData> {
        let comp_dir = env.destdir.join("usr/share/bash-completion/completions");
        fs::create_dir_all(&comp_dir)
            .map_err(|e| InvalidData::new(&format!("Failed to create bash completion dir: {}", e), None))?;

        let src_path = Path::new(src);
        if !src_path.exists() {
            return Err(InvalidData::new(&format!("Bash completion source {} not found", src), None));
        }

        let dest = comp_dir.join(dest_name);
        fs::copy(src_path, &dest)
            .map_err(|e| InvalidData::new(&format!("Failed to copy completion {}: {}", src, e), None))?;

        Ok(())
    }
}

/// Lua eclass functions
pub mod lua {
    
    use std::process::Command;
    
    /// Get Lua version
    pub fn get_version() -> String {
        let output = Command::new("lua")
            .arg("-v")
            .output();
        
        match output {
            Ok(out) if out.status.success() => {
                let version_str = String::from_utf8_lossy(&out.stdout);
                if let Some(line) = version_str.lines().next() {
                    if let Some(ver) = line.split_whitespace().nth(1) {
                        return ver.to_string();
                    }
                }
                "5.4".to_string()
            }
            _ => "5.4".to_string(),
        }
    }
    
    /// Get Lua implementation (usually just "lua")
    pub fn get_impl() -> String {
        std::env::var("LUA_IMPL").unwrap_or_else(|_| "lua".to_string())
    }
}

/// Qt6 eclass functions
pub mod qt6 {
    
    use std::process::Command;
    
    /// Get Qt6 binary directory
    pub fn get_bindir() -> String {
        let output = Command::new("qmake6")
            .arg("-query")
            .arg("QT_INSTALL_BINS")
            .output();
        
        match output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            }
            _ => {
                let output = Command::new("pkg-config")
                    .arg("--variable=bindir")
                    .arg("Qt6Core")
                    .output();
                    
                match output {
                    Ok(out) if out.status.success() => {
                        String::from_utf8_lossy(&out.stdout).trim().to_string()
                    }
                    _ => "/usr/lib64/qt6/bin".to_string(),
                }
            }
        }
    }
    
    /// Get Qt6 library directory
    pub fn get_libdir() -> String {
        let output = Command::new("qmake6")
            .arg("-query")
            .arg("QT_INSTALL_LIBS")
            .output();
        
        match output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout).trim().to_string()
            }
            _ => "/usr/lib64/qt6".to_string(),
        }
    }
}

impl Default for EclassManager {
    fn default() -> Self {
        Self::new()
    }
}
