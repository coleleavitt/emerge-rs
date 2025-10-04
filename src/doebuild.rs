// doebuild.rs -- Ebuild execution and build process

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use crate::exception::InvalidData;
use chrono;
use nix::unistd;
use crate::ebuild::EbuildEnvironment;
use crate::ebuild::src_uri::expand_string;

/// Represents an ebuild file and its metadata
#[derive(Debug, Clone)]
pub struct Ebuild {
    pub path: PathBuf,
    pub category: String,
    pub package: String,
    pub version: String,
    pub metadata: EbuildMetadata,
}

/// Ebuild metadata extracted from the ebuild file
#[derive(Debug, Clone)]
pub struct EbuildMetadata {
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub src_uri: Vec<String>,
    pub license: Option<String>,
    pub slot: String,
    pub keywords: Vec<String>,
    pub iuse: Vec<String>,
    pub depend: Vec<crate::dep::Atom>,
    pub rdepend: Vec<crate::dep::Atom>,
    pub pdepend: Vec<crate::dep::Atom>,
    pub inherit: Vec<String>,
    pub eapi: String,
}

/// Build environment for ebuild execution
pub struct BuildEnv {
    pub workdir: PathBuf,
    pub sourcedir: PathBuf,
    pub builddir: PathBuf,
    pub destdir: PathBuf,
    pub portdir: PathBuf,
    pub distdir: PathBuf,
    pub use_flags: HashMap<String, bool>,
    pub env_vars: HashMap<String, String>,
    pub native_executor: crate::ebuild::NativePhaseExecutor,
    // Build environment management
    pub features: Vec<String>,
    pub sandbox_enabled: bool,
    pub user_privilege: BuildUser,
}

/// User privilege settings for builds
#[derive(Debug, Clone)]
pub enum BuildUser {
    Root,
    Portage { uid: u32, gid: u32 },
    Custom { uid: u32, gid: u32 },
}

/// Ebuild build phases
#[derive(Debug, Clone, Copy)]
pub enum BuildPhase {
    Setup,
    Fetch,
    Unpack,
    Prepare,
    Configure,
    Compile,
    Test,
    Install,
    Merge,
    Package,
}

impl Ebuild {
    /// Parse an ebuild file from path
    pub fn from_path(path: &Path) -> Result<Self, InvalidData> {
        Self::from_path_with_use(path, &std::collections::HashMap::new())
    }

    /// Parse an ebuild file from path with USE flags
    pub fn from_path_with_use(path: &Path, use_flags: &std::collections::HashMap<String, bool>) -> Result<Self, InvalidData> {
        if !path.exists() {
            return Err(InvalidData::new(&format!("Ebuild file not found: {}", path.display()), None));
        }

        let content = fs::read_to_string(path)
            .map_err(|e| InvalidData::new(&format!("Failed to read ebuild: {}", e), None))?;

        // Extract category/package/version from path
        // Path format: /usr/portage/category/package/package-version.ebuild
        let path_str = path.to_string_lossy();
        let parts: Vec<&str> = path_str.split('/').collect();

        if parts.len() < 4 {
            return Err(InvalidData::new("Invalid ebuild path format", None));
        }

        let category = parts[parts.len() - 3].to_string();
        let filename = parts.last().unwrap();
        let filename_no_ext = filename.trim_end_matches(".ebuild");

        let mut package = String::new();
        let mut version = String::new();
        
        let mut found_version = false;
        for (i, c) in filename_no_ext.chars().enumerate() {
            if !found_version && c == '-' {
                if let Some(next_char) = filename_no_ext.chars().nth(i + 1) {
                    if next_char.is_ascii_digit() {
                        package = filename_no_ext[..i].to_string();
                        version = filename_no_ext[i + 1..].to_string();
                        found_version = true;
                        break;
                    }
                }
            }
        }
        
        if package.is_empty() || version.is_empty() {
            return Err(InvalidData::new("Invalid ebuild filename format", None));
        }

        let metadata = Self::parse_metadata_with_use(&content, use_flags, &category, &package, &version)?;

        Ok(Ebuild {
            path: path.to_path_buf(),
            category,
            package,
            version,
            metadata,
        })
    }

    /// Parse ebuild metadata from content
    pub fn parse_metadata(content: &str) -> Result<EbuildMetadata, InvalidData> {
        Self::parse_metadata_with_use(content, &std::collections::HashMap::new(), "", "", "")
    }

    /// Parse ebuild metadata from content with USE flags
    pub fn parse_metadata_with_use(content: &str, use_flags: &std::collections::HashMap<String, bool>, category: &str, package: &str, version: &str) -> Result<EbuildMetadata, InvalidData> {
        let mut metadata = EbuildMetadata {
            description: None,
            homepage: None,
            src_uri: Vec::new(),
            license: None,
            slot: "0".to_string(),
            keywords: Vec::new(),
            iuse: Vec::new(),
            depend: Vec::new(),
            rdepend: Vec::new(),
            pdepend: Vec::new(),
            inherit: Vec::new(),
            eapi: "8".to_string(),
        };

        let mut variables = std::collections::HashMap::new();
        
        let (pv, pr) = if let Some(r_pos) = version.rfind("-r") {
            if version[r_pos + 2..].chars().all(|c| c.is_ascii_digit()) {
                (&version[..r_pos], &version[r_pos + 1..])
            } else {
                (version, "r0")
            }
        } else {
            (version, "r0")
        };
        
        variables.insert("PN".to_string(), package.to_string());
        variables.insert("PV".to_string(), pv.to_string());
        variables.insert("P".to_string(), format!("{}-{}", package, pv));
        variables.insert("PF".to_string(), format!("{}-{}", package, version));
        variables.insert("PR".to_string(), pr.to_string());
        variables.insert("CATEGORY".to_string(), category.to_string());
        variables.insert("CP".to_string(), format!("{}/{}", category, package));
        variables.insert("CPV".to_string(), format!("{}/{}-{}", category, package, version));
        
        for line in content.lines() {
            let line = line.trim();
            if let Some(eq_pos) = line.find('=') {
                if eq_pos > 0 && line.as_bytes().get(eq_pos - 1) != Some(&b'+') {
                    let var_name = line[..eq_pos].trim();
                    if let Some(value) = Self::extract_raw_value(line) {
                        let expanded_value = expand_string(&value, &variables).unwrap_or(value.clone());
                        variables.insert(var_name.to_string(), expanded_value);
                    }
                }
            }
        }
        
        if !variables.contains_key("MY_P") {
            variables.insert("MY_P".to_string(), format!("{}-{}", package, version));
        }
        if !variables.contains_key("MY_PN") {
            variables.insert("MY_PN".to_string(), package.to_string());
        }
        if !variables.contains_key("MY_PV") {
            variables.insert("MY_PV".to_string(), version.to_string());
        }

        // Second pass: parse metadata with variable expansion
        for line in content.lines() {
            let line = line.trim();
            if line.starts_with("EAPI=") {
                metadata.eapi = Self::extract_quoted_value(line).unwrap_or_else(|| "8".to_string());
            } else if line.starts_with("inherit ") {
                let inherit_line = &line[8..]; // Skip "inherit "
                metadata.inherit = inherit_line.split_whitespace().map(|s| s.to_string()).collect();
            } else if line.starts_with("DESCRIPTION=") {
                metadata.description = Self::extract_quoted_value(line);
            } else if line.starts_with("HOMEPAGE=") {
                metadata.homepage = Self::extract_quoted_value(line);
            } else if line.starts_with("SRC_URI=") || line.starts_with("SRC_URI+=") {
                let current_src_uri = Self::extract_array_value_with_vars(line, &variables);
                metadata.src_uri.extend(current_src_uri);
            } else if line.starts_with("LICENSE=") {
                metadata.license = Self::extract_quoted_value(line);
            } else if line.starts_with("SLOT=") {
                metadata.slot = Self::extract_quoted_value(line).unwrap_or_else(|| "0".to_string());
            } else if line.starts_with("KEYWORDS=") {
                metadata.keywords = Self::extract_array_value_with_vars(line, &variables);
            } else if line.starts_with("IUSE=") {
                metadata.iuse = Self::extract_array_value_with_vars(line, &variables);
            } else if line.starts_with("DEPEND=") {
                if let Some(dep_str) = Self::extract_raw_value_with_vars(line, &variables) {
                    metadata.depend = crate::dep::parse_dependencies_with_use(&dep_str, &use_flags).unwrap_or_default();
                }
            } else if line.starts_with("RDEPEND=") {
                if let Some(dep_str) = Self::extract_raw_value_with_vars(line, &variables) {
                    metadata.rdepend = crate::dep::parse_dependencies_with_use(&dep_str, &use_flags).unwrap_or_default();
                }
            } else if line.starts_with("PDEPEND=") {
                if let Some(dep_str) = Self::extract_raw_value_with_vars(line, &variables) {
                    metadata.pdepend = crate::dep::parse_dependencies_with_use(&dep_str, &use_flags).unwrap_or_default();
                }
            }
        }

        Ok(metadata)
    }

    fn expand_variables(input: &str, variables: &std::collections::HashMap<String, String>) -> String {
        expand_string(input, variables).unwrap_or_else(|_| input.to_string())
    }

    /// Extract quoted string value from bash variable assignment
    fn extract_quoted_value(line: &str) -> Option<String> {
        let eq_pos = line.find('=')?;
        let value_part = &line[eq_pos + 1..].trim();

        if value_part.len() >= 2 && value_part.starts_with('"') && value_part.ends_with('"') {
            Some(value_part[1..value_part.len() - 1].to_string())
        } else if value_part.len() >= 2 && value_part.starts_with('\'') && value_part.ends_with('\'') {
            Some(value_part[1..value_part.len() - 1].to_string())
        } else {
            Some(value_part.to_string())
        }
    }

    /// Extract raw value from bash variable assignment
    fn extract_raw_value_with_vars(line: &str, variables: &std::collections::HashMap<String, String>) -> Option<String> {
        let expanded_line = Self::expand_variables(line, variables);
        Self::extract_raw_value(&expanded_line)
    }

    fn extract_raw_value(line: &str) -> Option<String> {
        let eq_pos = line.find('=')?;
        let value_part = &line[eq_pos + 1..].trim();
        // Trim surrounding quotes if present
        let trimmed = value_part.trim_matches('"').trim_matches('\'');
        Some(trimmed.to_string())
    }

    /// Extract array value from bash variable assignment
    fn extract_array_value_with_vars(line: &str, variables: &std::collections::HashMap<String, String>) -> Vec<String> {
        let expanded_line = Self::expand_variables(line, variables);
        Self::extract_array_value(&expanded_line)
    }

    fn extract_array_value(line: &str) -> Vec<String> {
        let eq_pos = line.find('=');
        if eq_pos.is_none() {
            return Vec::new();
        }

        let eq_pos = eq_pos.unwrap();
        let mut value_part = line[eq_pos + 1..].trim().to_string();

        // Handle += syntax - skip the +
        if eq_pos > 0 && line.as_bytes()[eq_pos - 1] == b'+' {
            // This is +=, skip the + in value_part
            if value_part.starts_with('+') {
                value_part = value_part[1..].trim().to_string();
            }
        }

        // Trim again in case there are extra spaces
        value_part = value_part.trim().to_string();

        if value_part.starts_with('(') && value_part.ends_with(')') {
            // Array format: SRC_URI=( "uri1" "uri2" )
            let inner = &value_part[1..value_part.len() - 1];
            inner.split_whitespace()
                .map(|s| s.trim_matches('"').trim_matches('\'').to_string())
                .filter(|s| !s.is_empty())
                .collect()
        } else if &value_part == "\"" || &value_part == "'" {
            // Incomplete quoted string (multiline assignment)
            Vec::new()
        } else if value_part.starts_with('"') && value_part.ends_with('"') && value_part.len() >= 2 {
            // String format: SRC_URI="uri1 uri2"
            let inner = &value_part[1..value_part.len() - 1];
            if inner.is_empty() {
                Vec::new()
            } else {
                inner.split_whitespace()
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
        } else if value_part.starts_with('\'') && value_part.ends_with('\'') && value_part.len() >= 2 {
            // Single quoted string format: SRC_URI='uri1 uri2'
            let inner = &value_part[1..value_part.len() - 1];
            if inner.is_empty() {
                Vec::new()
            } else {
                inner.split_whitespace()
                    .map(|s| s.to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            }
        } else {
            // Unquoted format: SRC_URI=uri1
            vec![value_part.to_string()]
        }
    }

    /// Get the full package name (category/package-version)
    pub fn cpv(&self) -> String {
        format!("{}/{}-{}", self.category, self.package, self.version)
    }

    /// Get the category/package part
    pub fn cp(&self) -> String {
        format!("{}/{}", self.category, self.package)
    }
    
    /// Get the PF (package-version)
    pub fn pf(&self) -> String {
        format!("{}-{}", self.package, self.version)
    }
}

impl BuildEnv {
    /// Convert to EbuildEnvironment for use with Rust ebuild module
    pub fn to_ebuild_env(&self) -> EbuildEnvironment {
        use crate::ebuild::environment::EbuildEnvironment;
        
        let use_flags: Vec<String> = self.use_flags.iter()
            .filter(|&(_, &enabled)| enabled)
            .map(|(flag, _)| flag.clone())
            .collect();
            
        let mut env = EbuildEnvironment::new(
            self.workdir.clone(),
            use_flags
        );
        
        // Copy environment variables
        for (key, value) in &self.env_vars {
            env.set(key.clone(), value.clone());
        }
        
        // Set directories
        env.sourcedir = self.sourcedir.clone();
        env.destdir = self.destdir.clone();
        env.builddir = self.builddir.clone();
        
        env
    }

    /// Extract archive filenames from SRC_URI
    fn extract_archive_names(src_uri: &[String]) -> String {
        let mut archives = Vec::new();

        for uri in src_uri {
            // Extract filename from URI (last component after /)
            if let Some(filename) = uri.split('/').last() {
                // Remove query parameters if present
                let filename = filename.split('?').next().unwrap_or(filename);
                // Remove fragment if present
                let filename = filename.split('#').next().unwrap_or(filename);
                archives.push(filename.to_string());
            }
        }

        archives.join(" ")
    }

    /// Extract archive filenames from expanded SRC_URI string
    fn extract_archive_names_from_string(src_uri: &str) -> String {
        

        let uri_map = Self::parse_src_uri_for_archives(src_uri);
        uri_map.keys().cloned().collect::<Vec<_>>().join(" ")
    }

    /// Parse SRC_URI to extract archive names (simplified version of parse_src_uri)
    fn parse_src_uri_for_archives(src_uri: &str) -> HashMap<String, Vec<String>> {
        let mut uri_map = HashMap::new();
        let mut current_uris = Vec::new();
        let mut current_filename = None;

        for token in src_uri.split_whitespace() {
            if token.contains("://") {
                // This is a URI
                current_uris.push(token.to_string());
            } else if token == "->" {
                // Next token will be the filename
                continue;
            } else {
                // This is a filename (after ->)
                if let Some(filename) = current_filename.take() {
                    uri_map.insert(filename, current_uris.clone());
                }
                current_filename = Some(token.to_string());
                current_uris.clear();
            }
        }

        // Handle the last set
        if let Some(filename) = current_filename {
            uri_map.insert(filename, current_uris);
        } else if !current_uris.is_empty() {
            // No explicit filename, extract from URI
            for uri in current_uris {
                if let Some(filename) = Self::extract_filename_from_uri(&uri) {
                    uri_map.insert(filename, vec![uri]);
                }
            }
        }

        uri_map
    }

    /// Extract filename from URI
    fn extract_filename_from_uri(uri: &str) -> Option<String> {
        uri.split('/').last()
            .and_then(|s| s.split('?').next())
            .and_then(|s| s.split('#').next())
            .map(|s| s.to_string())
    }

    /// Expand variables in a string using ebuild information
    fn expand_variables(input: &str, ebuild: &Ebuild) -> String {
        let mut result = input.to_string();

        // Replace common variables
        result = result.replace("${PN}", &ebuild.package);
        result = result.replace("${P}", &format!("{}-{}", ebuild.package, ebuild.version));
        result = result.replace("${PV}", &ebuild.version);
        result = result.replace("${PF}", &ebuild.pf());
        result = result.replace("${CATEGORY}", &ebuild.category);

        // Handle ${PN} without braces
        result = result.replace("$PN", &ebuild.package);
        result = result.replace("$P", &format!("{}-{}", ebuild.package, ebuild.version));
        result = result.replace("$PV", &ebuild.version);
        result = result.replace("$PF", &ebuild.pf());
        result = result.replace("$CATEGORY", &ebuild.category);

        result
    }

    /// Create a new build environment for an ebuild
    pub fn new(ebuild: &Ebuild, portdir: &Path, distdir: &Path, use_flags: HashMap<String, bool>, features: Vec<String>) -> Self {
        // Use PORTAGE_TMPDIR/portage like Portage does
        // Default to /var/tmp if not set (see portage/cnf/make.globals)
        let portage_tmpdir = std::env::var("PORTAGE_TMPDIR").unwrap_or_else(|_| "/var/tmp".to_string());
        let build_prefix = PathBuf::from(&portage_tmpdir).join("portage");
        
        // PORTAGE_BUILDDIR = BUILD_PREFIX/CATEGORY/PF
        let portage_builddir = build_prefix.join(&ebuild.category).join(&ebuild.pf());
        
        // Set up directories like Portage does:
        // WORKDIR = PORTAGE_BUILDDIR/work
        // D = PORTAGE_BUILDDIR/image/
        let workdir = portage_builddir.join("work");
        
        // Extract PV (version without revision) from the full version
        let pv = if let Some((_, ver, _)) = crate::versions::pkgsplit(&format!("{}-{}", ebuild.package, ebuild.version)) {
            ver
        } else {
            ebuild.version.clone()
        };
        
        let sourcedir = workdir.join(format!("{}-{}", ebuild.package, pv));
        let builddir = portage_builddir.join("build");
        let destdir = portage_builddir.join("image");

        // Get the ebuild's directory (O) and set up FILESDIR
        let ebuild_dir = ebuild.path.parent().unwrap_or(Path::new("."));
        let filesdir = portage_builddir.join("files");
        let temp_dir = portage_builddir.join("temp");
        
        let mut env_vars = HashMap::new();
        env_vars.insert("PORTAGE_TMPDIR".to_string(), portage_tmpdir.clone());
        env_vars.insert("BUILD_PREFIX".to_string(), build_prefix.to_string_lossy().to_string());
        env_vars.insert("PORTAGE_BUILDDIR".to_string(), portage_builddir.to_string_lossy().to_string());
        env_vars.insert("WORKDIR".to_string(), workdir.to_string_lossy().to_string());
        env_vars.insert("S".to_string(), sourcedir.to_string_lossy().to_string());
        env_vars.insert("BUILD_DIR".to_string(), builddir.to_string_lossy().to_string());
        env_vars.insert("D".to_string(), format!("{}/", destdir.to_string_lossy())); // D ends with /
        env_vars.insert("T".to_string(), temp_dir.to_string_lossy().to_string());
        env_vars.insert("PORTDIR".to_string(), portdir.to_string_lossy().to_string());
        env_vars.insert("DISTDIR".to_string(), distdir.to_string_lossy().to_string());
        env_vars.insert("EBUILD".to_string(), ebuild.path.to_string_lossy().to_string());
        env_vars.insert("O".to_string(), ebuild_dir.to_string_lossy().to_string());
        env_vars.insert("FILESDIR".to_string(), filesdir.to_string_lossy().to_string());

        // Set PATH to include standard directories
        env_vars.insert("PATH".to_string(), "/usr/bin:/bin:/usr/sbin:/sbin".to_string());
        env_vars.insert("PV".to_string(), pv.clone());
        env_vars.insert("PN".to_string(), ebuild.package.clone());
        env_vars.insert("P".to_string(), format!("{}-{}", ebuild.package, pv));
        env_vars.insert("PF".to_string(), ebuild.pf());
        env_vars.insert("CATEGORY".to_string(), ebuild.category.clone());

        // Set SRC_URI and A variables
        let src_uri_value = ebuild.metadata.src_uri.join(" ");
        // Expand variables in SRC_URI
        let expanded_src_uri = Self::expand_variables(&src_uri_value, &ebuild);
        env_vars.insert("SRC_URI".to_string(), expanded_src_uri.clone());
        let a_value = Self::extract_archive_names_from_string(&expanded_src_uri);
        env_vars.insert("A".to_string(), a_value);

        // Determine sandbox and user settings based on features
        let sandbox_enabled = features.contains(&"sandbox".to_string());
        let user_privilege = Self::determine_build_user(&features);

        // Set up sandbox environment variables if enabled
        if sandbox_enabled {
            env_vars.insert("SANDBOX_ON".to_string(), "1".to_string());
            // Add sandbox-specific environment variables
            env_vars.insert("SANDBOX_WRITE".to_string(), format!("{}:{}", destdir.display(), portage_builddir.display()));
            env_vars.insert("SANDBOX_PREDICT".to_string(), "/proc:/dev:/sys".to_string());
        }

        // Create a temporary native executor (will be replaced in doebuild)
        let native_executor = crate::ebuild::NativePhaseExecutor::new(
            &Vec::new(),
            &sourcedir
        );
        
        BuildEnv {
            workdir: portage_builddir,  // Use PORTAGE_BUILDDIR as workdir
            sourcedir,
            builddir,
            destdir,
            portdir: portdir.to_path_buf(),
            distdir: distdir.to_path_buf(),
            use_flags,
            env_vars,
            native_executor,
            features,
            sandbox_enabled,
            user_privilege,
        }
    }

    /// Determine which user to run builds as based on features
    fn determine_build_user(features: &[String]) -> BuildUser {
        // Check if we should run as portage user
        if features.contains(&"userpriv".to_string()) || features.contains(&"usersandbox".to_string()) {
            // Try to get portage user/group IDs
            if let (Some(uid), Some(gid)) = (Self::get_portage_uid(), Self::get_portage_gid()) {
                return BuildUser::Portage { uid, gid };
            }
        }

        // Default to root for now (in testing/development)
        // In production, this should be configurable
        BuildUser::Root
    }

    /// Get portage user ID
    fn get_portage_uid() -> Option<u32> {
        // Try to get portage user ID
        match std::process::Command::new("id").args(&["-u", "portage"]).output() {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().parse().ok()
            }
            _ => None,
        }
    }

    /// Get portage group ID
    fn get_portage_gid() -> Option<u32> {
        // Try to get portage group ID
        match std::process::Command::new("id").args(&["-g", "portage"]).output() {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().parse().ok()
            }
            _ => None,
        }
    }

    /// Set up the build environment directories
    pub fn setup(&self) -> Result<(), InvalidData> {
        use std::os::unix::fs::PermissionsExt;
        
        // Create base build directory with proper permissions BEFORE any user switching
        // This is PORTAGE_TMPDIR/portage (BUILD_PREFIX)
        let portage_tmpdir = std::env::var("PORTAGE_TMPDIR").unwrap_or_else(|_| "/var/tmp".to_string());
        let base_build_dir = PathBuf::from(&portage_tmpdir).join("portage");
        
        if !base_build_dir.exists() {
            fs::create_dir_all(&base_build_dir)
                .map_err(|e| InvalidData::new(&format!("Failed to create base build dir: {}", e), None))?;
            
            // Set sticky bit + world writable permissions so portage user can create subdirs
            fs::set_permissions(&base_build_dir, fs::Permissions::from_mode(0o1777))
                .map_err(|e| InvalidData::new(&format!("Failed to set base build dir permissions: {}", e), None))?;
        }
        
        // Create PORTAGE_BUILDDIR (workdir in our struct)
        fs::create_dir_all(&self.workdir)
            .map_err(|e| {
                // Provide helpful error message if permission denied
                if e.kind() == std::io::ErrorKind::PermissionDenied {
                    InvalidData::new(&format!(
                        "Failed to create PORTAGE_BUILDDIR: {}\n\
                         You may need to run emerge-rs with sudo, or ensure {} is writable by your user.\n\
                         Alternatively, set PORTAGE_TMPDIR to a directory you own.",
                        e, base_build_dir.display()
                    ), None)
                } else {
                    InvalidData::new(&format!("Failed to create PORTAGE_BUILDDIR: {}", e), None)
                }
            })?;
        
        // Set ownership and permissions on PORTAGE_BUILDDIR if using portage user
        if let BuildUser::Portage { uid, gid } = &self.user_privilege {
            let output = Command::new("chown")
                .arg("-R")
                .arg(format!("{}:{}", uid, gid))
                .arg(&self.workdir)
                .output();
            
            if let Err(e) = output {
                eprintln!("Warning: Failed to chown PORTAGE_BUILDDIR to portage: {}", e);
            } else if let Ok(output) = output {
                if !output.status.success() {
                    eprintln!("Warning: chown failed: {}", String::from_utf8_lossy(&output.stderr));
                }
            }
        }
        
        fs::set_permissions(&self.workdir, fs::Permissions::from_mode(0o755))
            .map_err(|e| InvalidData::new(&format!("Failed to set PORTAGE_BUILDDIR permissions: {}", e), None))?;
        
        // Create subdirectories (WORKDIR, BUILD_DIR, D, T)
        // Note: sourcedir is under WORKDIR which is under PORTAGE_BUILDDIR
        let actual_workdir = self.workdir.join("work");
        fs::create_dir_all(&actual_workdir)
            .map_err(|e| InvalidData::new(&format!("Failed to create WORKDIR: {}", e), None))?;
        fs::create_dir_all(&self.builddir)
            .map_err(|e| InvalidData::new(&format!("Failed to create BUILD_DIR: {}", e), None))?;
        fs::create_dir_all(&self.destdir)
            .map_err(|e| InvalidData::new(&format!("Failed to create D: {}", e), None))?;
        
        let temp_dir = self.workdir.join("temp");
        fs::create_dir_all(&temp_dir)
            .map_err(|e| InvalidData::new(&format!("Failed to create T: {}", e), None))?;
        
        // Create FILESDIR symlink to real files directory (like Portage does)
        // FILESDIR = PORTAGE_BUILDDIR/files -> $O/files
        let filesdir = self.workdir.join("files");
        if let Some(o_dir) = self.env_vars.get("O") {
            let real_filesdir = PathBuf::from(o_dir).join("files");
            // Remove old symlink if exists
            let _ = fs::remove_file(&filesdir);
            // Create symlink if the real files directory exists
            if real_filesdir.exists() {
                std::os::unix::fs::symlink(&real_filesdir, &filesdir)
                    .map_err(|e| InvalidData::new(&format!("Failed to create FILESDIR symlink: {}", e), None))?;
            }
        }

        // Set up sandbox if enabled
        if self.sandbox_enabled {
            self.setup_sandbox()?;
        }

        // Set up user privileges (ownership of directories)
        self.setup_user_privileges()?;

        Ok(())
    }

    /// Set up sandbox environment
    fn setup_sandbox(&self) -> Result<(), InvalidData> {
        // Check if sandbox is available
        if !std::process::Command::new("sandbox").arg("--version").output().is_ok() {
            if self.features.contains(&"strict".to_string()) {
                return Err(InvalidData::new("Sandbox requested but not available", None));
            } else {
                eprintln!("Warning: Sandbox requested but not available, continuing without sandbox");
                return Ok(());
            }
        }

        // Sandbox is already configured via environment variables in new()
        // The actual sandboxing happens when executing commands
        Ok(())
    }

    /// Set up user privileges for the build
    fn setup_user_privileges(&self) -> Result<(), InvalidData> {
        match &self.user_privilege {
            BuildUser::Root => {
                // No special setup needed for root
                Ok(())
            }
            BuildUser::Portage { uid, gid } => {
                // Set ownership of build directories to portage user
                self.set_directory_ownership(uid, gid)?;
                Ok(())
            }
            BuildUser::Custom { uid, gid } => {
                // Set ownership of build directories to custom user
                self.set_directory_ownership(uid, gid)?;
                Ok(())
            }
        }
    }

    /// Set ownership of build directories
    fn set_directory_ownership(&self, uid: &u32, gid: &u32) -> Result<(), InvalidData> {
        // Use chown to set ownership (requires root privileges)
        let dirs = [&self.workdir, &self.sourcedir, &self.builddir, &self.destdir];

        for dir in &dirs {
            if dir.exists() {
                let output = std::process::Command::new("chown")
                    .args(&["-R", &format!("{}:{}", uid, gid), &dir.to_string_lossy()])
                    .output()
                    .map_err(|e| InvalidData::new(&format!("Failed to set ownership: {}", e), None))?;

                if !output.status.success() {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    eprintln!("Warning: Failed to set ownership of {}: {}", dir.display(), stderr);
                    // Don't fail hard, just warn
                }
            }
        }

        Ok(())
    }

    pub async fn execute_phase(&self, ebuild: &Ebuild, phase: BuildPhase) -> Result<(), InvalidData> {
        match phase {
            BuildPhase::Setup => self.phase_setup().await,
            BuildPhase::Fetch => self.phase_fetch(ebuild).await,
            BuildPhase::Unpack => self.phase_unpack(ebuild).await,
            BuildPhase::Prepare => self.phase_prepare(ebuild).await,
            BuildPhase::Configure => self.phase_configure(ebuild).await,
            BuildPhase::Compile => self.phase_compile(ebuild).await,
            BuildPhase::Test => self.phase_test(ebuild).await,
            BuildPhase::Install => self.phase_install(ebuild).await,
            BuildPhase::Merge => self.phase_merge(ebuild).await,
            BuildPhase::Package => self.phase_package(ebuild).await,
        }
    }

    async fn phase_setup(&self) -> Result<(), InvalidData> {
        println!("Setting up build environment...");

        // Note: We intentionally do NOT switch users here because:
        // 1. Once we drop from root to portage, we can't regain root for the merge phase
        // 2. The merge phase requires root to install files to system directories
        // TODO: Implement proper privilege separation using fork/exec
        // self.switch_to_build_user()?;

        Ok(())
    }
    
    async fn phase_fetch(&self, ebuild: &Ebuild) -> Result<(), InvalidData> {
        println!("Fetching sources for {}...", ebuild.cpv());

        let env = self.to_ebuild_env();
        self.native_executor.fetch(&env)
    }

    async fn phase_unpack(&self, ebuild: &Ebuild) -> Result<(), InvalidData> {
        println!("Unpacking sources for {}...", ebuild.cpv());

        // Use native Rust phase executor
        let env = self.to_ebuild_env();
        self.native_executor.src_unpack(&env)
    }

    async fn phase_prepare(&self, ebuild: &Ebuild) -> Result<(), InvalidData> {
        println!("Preparing sources for {}...", ebuild.cpv());

        // Use native Rust phase executor
        let env = self.to_ebuild_env();
        self.native_executor.src_prepare(&env)
    }

    async fn phase_configure(&self, ebuild: &Ebuild) -> Result<(), InvalidData> {
        println!("Configuring {}...", ebuild.cpv());

        // Use native Rust phase executor
        let env = self.to_ebuild_env();
        self.native_executor.src_configure(&env)
    }
    
    async fn phase_compile(&self, ebuild: &Ebuild) -> Result<(), InvalidData> {
        println!("Compiling {}...", ebuild.cpv());

        // Use native Rust phase executor
        let env = self.to_ebuild_env();
        self.native_executor.src_compile(&env)
    }
    
    async fn phase_test(&self, ebuild: &Ebuild) -> Result<(), InvalidData> {
        println!("Testing {}...", ebuild.cpv());

        // Use native Rust phase executor
        let env = self.to_ebuild_env();
        self.native_executor.src_test(&env)
    }
    
    async fn phase_install(&self, ebuild: &Ebuild) -> Result<(), InvalidData> {
        println!("Installing {}...", ebuild.cpv());

        let env = self.to_ebuild_env();
        self.native_executor.src_install(&env)
    }
    
    async fn phase_merge(&self, ebuild: &Ebuild) -> Result<(), InvalidData> {
        println!("Merging {} to filesystem...", ebuild.cpv());

        let env = self.to_ebuild_env();
        let root = std::env::var("ROOT").unwrap_or_else(|_| "/".to_string());
        self.native_executor.merge(&env, &root)
    }
    
    async fn phase_package(&self, ebuild: &Ebuild) -> Result<(), InvalidData> {
        println!("Packaging {}...", ebuild.cpv());

        // Create binary package (.tbz2)
        self.create_binary_package(ebuild, "gentoo").await // TODO: get actual repository
    }

    /// Create a binary package (.tbz2 file)
    async fn create_binary_package(&self, ebuild: &Ebuild, repository: &str) -> Result<(), InvalidData> {
        use tokio::process::Command;

        let cpv = ebuild.cpv();
        let pkgdir = format!("/usr/portage/packages");

        // Ensure packages directory exists
        tokio::fs::create_dir_all(&pkgdir)
            .await
            .map_err(|e| InvalidData::new(&format!("Failed to create packages directory: {}", e), None))?;

        let tbz2_path = format!("{}/{}.tbz2", pkgdir, cpv);

        // Create tar.bz2 archive of the installed files
        let tar_cmd = Command::new("tar")
            .args(&["-cjf", &tbz2_path, "-C", &self.destdir.to_string_lossy(), "."])
            .status()
            .await
            .map_err(|e| InvalidData::new(&format!("Failed to create tar archive: {}", e), None))?;

        if !tar_cmd.success() {
            return Err(InvalidData::new("tar command failed", None));
        }

        // Create XPAK metadata
        let mut xpak_data = std::collections::HashMap::new();

        // Add basic metadata
        xpak_data.insert("SLOT".to_string(), ebuild.metadata.slot.as_bytes().to_vec());
        xpak_data.insert("repository".to_string(), repository.as_bytes().to_vec());

        if let Some(description) = &ebuild.metadata.description {
            xpak_data.insert("DESCRIPTION".to_string(), description.as_bytes().to_vec());
        }

        if let Some(license) = &ebuild.metadata.license {
            xpak_data.insert("LICENSE".to_string(), license.as_bytes().to_vec());
        }

        // Add USE flags (simplified)
        let use_flags: Vec<String> = self.use_flags.iter()
            .filter(|&(_, &enabled)| enabled)
            .map(|(flag, _)| flag.clone())
            .collect();
        if !use_flags.is_empty() {
            xpak_data.insert("USE".to_string(), use_flags.join(" ").as_bytes().to_vec());
        }

        // Add keywords
        if !ebuild.metadata.keywords.is_empty() {
            xpak_data.insert("KEYWORDS".to_string(), ebuild.metadata.keywords.join(" ").as_bytes().to_vec());
        }

        // Create XPAK data
        let xpak_bytes = crate::xpak::xpak_mem(&xpak_data);

        // Append XPAK data to the .tbz2 file
        use std::fs::OpenOptions;
        use std::io::Write;

        let mut file = OpenOptions::new()
            .append(true)
            .open(&tbz2_path)
            .map_err(|e| InvalidData::new(&format!("Failed to open tbz2 file for appending: {}", e), None))?;

        file.write_all(&xpak_bytes)
            .map_err(|e| InvalidData::new(&format!("Failed to append XPAK data: {}", e), None))?;

        println!("Created binary package: {}", tbz2_path);
        Ok(())
    }

    /// Switch to portage user if running as root
    fn switch_to_build_user(&self) -> Result<(), InvalidData> {
        match &self.user_privilege {
            BuildUser::Root => {
                // Already running as root, nothing to do
                Ok(())
            }
            BuildUser::Portage { uid, gid } => {
                // Check if we're running as root
                if !unistd::Uid::effective().is_root() {
                    return Ok(());
                }

                println!("Switching to portage user for build (uid: {}, gid: {})...", uid, gid);

                // Switch to portage user
                if let Err(e) = unistd::setgid(unistd::Gid::from_raw(*gid)) {
                    eprintln!("Warning: Failed to setgid to portage group: {}, continuing as root", e);
                    return Ok(());
                }
                if let Err(e) = unistd::setuid(unistd::Uid::from_raw(*uid)) {
                    eprintln!("Warning: Failed to setuid to portage user: {}, continuing as root", e);
                    return Ok(());
                }
                println!("Switched to portage user");
                Ok(())
            }
            BuildUser::Custom { uid, gid } => {
                // Check if we're running as root
                if !unistd::Uid::effective().is_root() {
                    return Ok(());
                }

                println!("Switching to custom user for build (uid: {}, gid: {})...", uid, gid);

                // Switch to custom user
                if let Err(e) = unistd::setgid(unistd::Gid::from_raw(*gid)) {
                    eprintln!("Warning: Failed to setgid to custom group: {}, continuing as root", e);
                    return Ok(());
                }
                if let Err(e) = unistd::setuid(unistd::Uid::from_raw(*uid)) {
                    eprintln!("Warning: Failed to setuid to custom user: {}, continuing as root", e);
                    return Ok(());
                }
                println!("Switched to custom user");
                Ok(())
            }
        }
    }


}

/// Set up build logging for a package
fn setup_build_logging(ebuild: &Ebuild, build_env: &BuildEnv) -> Result<Option<std::fs::File>, InvalidData> {
    use std::fs;

    // Create log in $T (PORTAGE_BUILDDIR/temp) like Portage does
    let temp_dir = build_env.workdir.join("temp");
    fs::create_dir_all(&temp_dir)
        .map_err(|e| InvalidData::new(&format!("Failed to create temp directory: {}", e), None))?;

    // Create log file - Portage uses build.log in $T  
    let log_path = temp_dir.join("build.log");
    let log_file = fs::File::create(&log_path)
        .map_err(|e| InvalidData::new(&format!("Failed to create log file {}: {}", log_path.display(), e), None))?;

    println!("Build log: {}", log_path.display());
    Ok(Some(log_file))
}

/// Main doebuild function to build a package from ebuild
pub async fn doebuild(ebuild_path: &Path, phases: &[BuildPhase], use_flags: HashMap<String, bool>, features: Vec<String>, portdir: &Path, distdir: &Path) -> Result<BuildEnv, InvalidData> {
    let ebuild = Ebuild::from_path_with_use(ebuild_path, &use_flags)?;

    println!("Building {} from {}", ebuild.cpv(), ebuild_path.display());
    println!("Ebuild metadata: {:?}", ebuild.metadata);

    let mut build_env = BuildEnv::new(&ebuild, portdir, distdir, use_flags, features);
    println!("Build environment workdir: {}", build_env.workdir.display());
    println!("Build environment sourcedir: {}", build_env.sourcedir.display());

    // Create native phase executor based on INHERIT and EAPI
    let eapi = crate::ebuild::Eapi::from_str(&ebuild.metadata.eapi)
        .unwrap_or(crate::ebuild::Eapi::Eapi8);
    build_env.native_executor = crate::ebuild::NativePhaseExecutor::with_eapi(
        &ebuild.metadata.inherit,
        &build_env.sourcedir,
        eapi
    );

    // Setup directories first
    build_env.setup()?;
    
    // Set up build logging AFTER setup creates directories
    let mut log_file = setup_build_logging(&ebuild, &build_env)?;

    // Log build start
    if let Some(ref mut log_file) = log_file {
        use std::io::Write;
        let _ = writeln!(log_file, ">>> Build started for {} at {}", ebuild.cpv(), chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
    }

    for &phase in phases {
        println!("Executing phase: {:?}", phase);

        // Log phase start
        if let Some(ref mut log_file) = log_file {
            use std::io::Write;
            let _ = writeln!(log_file, ">>> Executing phase: {:?} at {}", phase, chrono::Utc::now().format("%H:%M:%S"));
        }

        build_env.execute_phase(&ebuild, phase).await?;

        // Log phase completion
        if let Some(ref mut log_file) = log_file {
            use std::io::Write;
            let _ = writeln!(log_file, ">>> Phase {:?} completed successfully", phase);
        }
    }

    // Log build completion
    if let Some(ref mut log_file) = log_file {
        use std::io::Write;
        let _ = writeln!(log_file, ">>> Build completed successfully for {} at {}", ebuild.cpv(), chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
    }

    println!("Build completed successfully for {}", ebuild.cpv());
    Ok(build_env)
}