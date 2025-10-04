// environment.rs - Ebuild environment management
use std::collections::HashMap;
use std::path::PathBuf;

/// Ebuild execution environment
#[derive(Debug, Clone)]
pub struct EbuildEnvironment {
    /// Environment variables
    pub vars: HashMap<String, String>,
    /// USE flags that are enabled
    pub use_flags: Vec<String>,
    /// Current working directory
    pub workdir: PathBuf,
    /// Source directory
    pub sourcedir: PathBuf,
    /// Destination directory for installation
    pub destdir: PathBuf,
    /// Build directory
    pub builddir: PathBuf,
    /// EAPI version
    pub eapi: String,
}

impl EbuildEnvironment {
    /// Create a new ebuild environment
    pub fn new(workdir: PathBuf, use_flags: Vec<String>) -> Self {
        let sourcedir = workdir.join("work");
        let destdir = workdir.join("image");
        let builddir = workdir.join("build");
        
        let mut vars = HashMap::new();
        vars.insert("WORKDIR".to_string(), workdir.to_string_lossy().to_string());
        vars.insert("S".to_string(), sourcedir.to_string_lossy().to_string());
        vars.insert("D".to_string(), destdir.to_string_lossy().to_string());
        vars.insert("BUILD_DIR".to_string(), builddir.to_string_lossy().to_string());
        
        let path = std::env::var("PATH").unwrap_or_else(|_| 
            "/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin".to_string()
        );
        vars.insert("PATH".to_string(), path);
        
        EbuildEnvironment {
            vars,
            use_flags,
            workdir,
            sourcedir,
            destdir,
            builddir,
            eapi: "8".to_string(), // Default to EAPI 8
        }
    }

    /// Check if a USE flag is enabled
    pub fn use_flag_enabled(&self, flag: &str) -> bool {
        self.use_flags.contains(&flag.to_string())
    }

    /// Get an environment variable
    pub fn get(&self, key: &str) -> Option<&String> {
        self.vars.get(key)
    }

    /// Set an environment variable
    pub fn set(&mut self, key: String, value: String) {
        self.vars.insert(key, value);
    }

    /// Export environment variable to a format suitable for shell execution
    pub fn export_string(&self) -> String {
        let mut exports = String::new();
        for (key, value) in &self.vars {
            exports.push_str(&format!("export {}=\"{}\"\n", key, value));
        }
        exports
    }
}
