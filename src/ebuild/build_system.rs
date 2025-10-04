// build_system.rs - Detect and identify build systems
//
// Automatically detects the build system used by a package from:
// 1. INHERIT variable in ebuild (cmake, meson, etc.)
// 2. Files in source directory (CMakeLists.txt, meson.build, configure, etc.)

use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuildSystem {
    CMake,
    Meson,
    Autotools,
    Makefile,
    Cargo,
    Python,
    Go,
    Custom,
}

impl BuildSystem {
    /// Detect build system from ebuild INHERIT and source directory
    pub fn detect(inherit: &[String], source_dir: &Path) -> Self {
        // First check INHERIT
        for eclass in inherit {
            match eclass.as_str() {
                "cmake" | "cmake-utils" => return Self::CMake,
                "meson" => return Self::Meson,
                "autotools" | "autotools-utils" => return Self::Autotools,
                "cargo" => return Self::Cargo,
                "distutils-r1" | "python-r1" | "python-single-r1" => return Self::Python,
                "go-module" => return Self::Go,
                _ => {}
            }
        }
        
        // Check source directory for build files
        if source_dir.join("CMakeLists.txt").exists() {
            return Self::CMake;
        }
        
        if source_dir.join("meson.build").exists() {
            return Self::Meson;
        }
        
        if source_dir.join("configure").exists() || 
           source_dir.join("configure.ac").exists() ||
           source_dir.join("configure.in").exists() {
            return Self::Autotools;
        }
        
        if source_dir.join("Cargo.toml").exists() {
            return Self::Cargo;
        }
        
        if source_dir.join("setup.py").exists() || 
           source_dir.join("pyproject.toml").exists() {
            return Self::Python;
        }
        
        if source_dir.join("go.mod").exists() {
            return Self::Go;
        }
        
        if source_dir.join("Makefile").exists() || 
           source_dir.join("makefile").exists() {
            return Self::Makefile;
        }
        
        Self::Custom
    }
    
    /// Get default configure arguments for this build system
    pub fn default_configure_args(&self) -> Vec<String> {
        match self {
            Self::CMake => vec![
                "-DCMAKE_INSTALL_PREFIX=/usr".to_string(),
                "-DCMAKE_BUILD_TYPE=Release".to_string(),
                "-DCMAKE_INSTALL_LIBDIR=lib".to_string(),
            ],
            Self::Meson => vec![
                "--prefix=/usr".to_string(),
                "--sysconfdir=/etc".to_string(),
                "--localstatedir=/var".to_string(),
                "--libdir=lib".to_string(),
                "-Dbuildtype=plain".to_string(),
            ],
            Self::Autotools => vec![
                "--prefix=/usr".to_string(),
                "--sysconfdir=/etc".to_string(),
                "--localstatedir=/var".to_string(),
            ],
            _ => vec![],
        }
    }
    
    /// Get build command for this build system
    pub fn build_command(&self) -> (&'static str, Vec<&'static str>) {
        match self {
            Self::CMake => ("cmake", vec!["--build", "."]),
            Self::Meson => ("meson", vec!["compile", "-C", "build"]),
            Self::Autotools | Self::Makefile => ("make", vec![]),
            Self::Cargo => ("cargo", vec!["build", "--release"]),
            // Python packages should not build in native executor - they need ebuild phases
            // For now, return true to indicate no build needed (ebuild handles it)
            Self::Python => ("true", vec![]),
            Self::Go => ("true", vec![]),
            Self::Custom => ("make", vec![]),
        }
    }
    
    /// Get install command for this build system
    pub fn install_command(&self, destdir: &str) -> (&'static str, Vec<String>) {
        match self {
            Self::CMake => ("cmake", vec!["--install".to_string(), ".".to_string()]),
            Self::Meson => ("meson", vec!["install".to_string(), "-C".to_string(), "build".to_string()]),
            Self::Autotools | Self::Makefile => ("make", vec!["install".to_string()]),
            Self::Cargo => ("cargo", vec!["install".to_string(), "--root".to_string(), destdir.to_string()]),
            // Python/Go should use ebuild phases, not native
            Self::Python => ("true", vec![]),
            Self::Go => ("true", vec![]),
            Self::Custom => ("make", vec!["install".to_string()]),
        }
    }
    
    /// Get configure command for this build system
    pub fn configure_command(&self, args: &[String]) -> Option<(&'static str, Vec<String>)> {
        match self {
            Self::CMake => {
                let mut cmd_args = vec!["-B".to_string(), "build".to_string(), "-S".to_string(), ".".to_string()];
                cmd_args.extend_from_slice(args);
                Some(("cmake", cmd_args))
            }
            Self::Meson => {
                let mut cmd_args = vec!["setup".to_string(), "build".to_string()];
                cmd_args.extend_from_slice(args);
                Some(("meson", cmd_args))
            }
            Self::Autotools => {
                let mut cmd_args = vec![];
                cmd_args.extend_from_slice(args);
                Some(("./configure", cmd_args))
            }
            Self::Cargo | Self::Python | Self::Go | Self::Makefile | Self::Custom => None,
        }
    }
    
    /// Translate USE flag to build system option
    pub fn use_to_option(&self, use_flag: &str, option_name: Option<&str>, enabled: bool) -> Option<String> {
        let opt = option_name.unwrap_or(use_flag);
        
        match self {
            Self::CMake => {
                let value = if enabled { "ON" } else { "OFF" };
                Some(format!("-DWITH_{}={}", opt.to_uppercase(), value))
            }
            Self::Meson => {
                let value = if enabled { "enabled" } else { "disabled" };
                Some(format!("-D{}={}", opt, value))
            }
            Self::Autotools => {
                if enabled {
                    Some(format!("--enable-{}", opt))
                } else {
                    Some(format!("--disable-{}", opt))
                }
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_detect_from_inherit() {
        let inherit = vec!["cmake".to_string()];
        let bs = BuildSystem::detect(&inherit, Path::new("/tmp"));
        assert_eq!(bs, BuildSystem::CMake);
        
        let inherit = vec!["meson".to_string()];
        let bs = BuildSystem::detect(&inherit, Path::new("/tmp"));
        assert_eq!(bs, BuildSystem::Meson);
    }
    
    #[test]
    fn test_use_to_option() {
        let bs = BuildSystem::CMake;
        assert_eq!(bs.use_to_option("ssl", None, true), Some("-DWITH_SSL=ON".to_string()));
        assert_eq!(bs.use_to_option("ssl", None, false), Some("-DWITH_SSL=OFF".to_string()));
        
        let bs = BuildSystem::Meson;
        assert_eq!(bs.use_to_option("ssl", None, true), Some("-Dssl=enabled".to_string()));
        assert_eq!(bs.use_to_option("ssl", None, false), Some("-Dssl=disabled".to_string()));
        
        let bs = BuildSystem::Autotools;
        assert_eq!(bs.use_to_option("ssl", None, true), Some("--enable-ssl".to_string()));
        assert_eq!(bs.use_to_option("ssl", None, false), Some("--disable-ssl".to_string()));
    }
}
