// helpers.rs - Core ebuild helper functions
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use crate::exception::InvalidData;
use super::environment::EbuildEnvironment;

/// Die with an error message
pub fn die(env: &EbuildEnvironment, message: &str) -> ! {
    eprintln!("ERROR: {}", message);
    std::process::exit(1);
}

/// Check if USE flag is enabled
pub fn use_enabled(env: &EbuildEnvironment, flag: &str) -> bool {
    env.use_flag_enabled(flag)
}

/// Use flag with exec (returns value for command substitution)
pub fn usex(env: &EbuildEnvironment, flag: &str, enabled_val: &str, disabled_val: &str) -> String {
    if use_enabled(env, flag) {
        enabled_val.to_string()
    } else {
        disabled_val.to_string()
    }
}

/// Check if item is in list
pub fn has(item: &str, list: &[&str]) -> bool {
    list.contains(&item)
}

/// Echo info message
pub fn einfo(message: &str) {
    println!(" * {}", message);
}

/// Echo warning message
pub fn ewarn(message: &str) {
    eprintln!(" * WARNING: {}", message);
}

/// Echo error message
pub fn eerror(message: &str) {
    eprintln!(" * ERROR: {}", message);
}

/// Echo QA warning
pub fn eqawarn(message: &str) {
    eprintln!(" * QA Notice: {}", message);
}

/// Begin an operation
pub fn ebegin(message: &str) {
    print!(" * {} ...", message);
    std::io::Write::flush(&mut std::io::stdout()).ok();
}

/// End an operation
pub fn eend(exit_code: i32, message: Option<&str>) {
    if exit_code == 0 {
        println!(" [ ok ]");
    } else {
        println!(" [ !! ]");
        if let Some(msg) = message {
            eerror(msg);
        }
    }
}

/// Install binary files
pub fn dobin(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    let dest_dir = env.destdir.join("usr/bin");
    fs::create_dir_all(&dest_dir)
        .map_err(|e| InvalidData::new(&format!("Failed to create bin directory: {}", e), None))?;
    
    for file in files {
        let src = Path::new(file);
        if !src.exists() {
            return Err(InvalidData::new(&format!("dobin: {} not found", file), None));
        }
        
        let filename = src.file_name()
            .ok_or_else(|| InvalidData::new(&format!("Invalid filename: {}", file), None))?;
        let dest = dest_dir.join(filename);
        
        fs::copy(src, &dest)
            .map_err(|e| InvalidData::new(&format!("Failed to copy {}: {}", file, e), None))?;
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dest)?.permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&dest, perms)?;
        }
    }
    
    Ok(())
}

/// Install data files
pub fn doins(env: &EbuildEnvironment, files: &[&str], subdir: Option<&str>) -> Result<(), InvalidData> {
    let dest_base = if let Some(sub) = subdir {
        env.destdir.join("usr/share").join(sub)
    } else {
        env.destdir.join("usr/share")
    };
    
    fs::create_dir_all(&dest_base)
        .map_err(|e| InvalidData::new(&format!("Failed to create directory: {}", e), None))?;
    
    for file in files {
        let src = Path::new(file);
        if !src.exists() {
            return Err(InvalidData::new(&format!("doins: {} not found", file), None));
        }
        
        let filename = src.file_name()
            .ok_or_else(|| InvalidData::new(&format!("Invalid filename: {}", file), None))?;
        let dest = dest_base.join(filename);
        
        fs::copy(src, &dest)
            .map_err(|e| InvalidData::new(&format!("Failed to copy {}: {}", file, e), None))?;
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dest)?.permissions();
            perms.set_mode(0o644);
            fs::set_permissions(&dest, perms)?;
        }
    }
    
    Ok(())
}

/// Install documentation
pub fn dodoc(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    let pf = env.get("PF").map(|s| s.as_str()).unwrap_or("unknown");
    let doc_dir = env.destdir.join("usr/share/doc").join(pf);
    
    fs::create_dir_all(&doc_dir)
        .map_err(|e| InvalidData::new(&format!("Failed to create doc directory: {}", e), None))?;
    
    for file in files {
        let src = Path::new(file);
        if !src.exists() {
            return Err(InvalidData::new(&format!("dodoc: {} not found", file), None));
        }
        
        let filename = src.file_name()
            .ok_or_else(|| InvalidData::new(&format!("Invalid filename: {}", file), None))?;
        let dest = doc_dir.join(filename);
        
        fs::copy(src, &dest)
            .map_err(|e| InvalidData::new(&format!("Failed to copy {}: {}", file, e), None))?;
    }
    
    Ok(())
}

/// Install man pages
pub fn doman(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    for file in files {
        let src = Path::new(file);
        if !src.exists() {
            return Err(InvalidData::new(&format!("doman: {} not found", file), None));
        }
        
        // Extract section from filename (e.g., foo.1 -> section 1)
        let filename = src.file_name()
            .ok_or_else(|| InvalidData::new(&format!("Invalid filename: {}", file), None))?
            .to_string_lossy();
        
        let section = filename.split('.').last()
            .ok_or_else(|| InvalidData::new(&format!("Cannot determine man section for {}", file), None))?;
        
        let man_dir = env.destdir.join(format!("usr/share/man/man{}", section));
        fs::create_dir_all(&man_dir)
            .map_err(|e| InvalidData::new(&format!("Failed to create man directory: {}", e), None))?;
        
        let dest = man_dir.join(filename.to_string());
        fs::copy(src, &dest)
            .map_err(|e| InvalidData::new(&format!("Failed to copy {}: {}", file, e), None))?;
    }
    
    Ok(())
}

/// Install with new name
pub fn newbin(env: &EbuildEnvironment, src: &str, dest_name: &str) -> Result<(), InvalidData> {
    let src_path = Path::new(src);
    if !src_path.exists() {
        return Err(InvalidData::new(&format!("newbin: {} not found", src), None));
    }
    
    let dest_dir = env.destdir.join("usr/bin");
    fs::create_dir_all(&dest_dir)
        .map_err(|e| InvalidData::new(&format!("Failed to create bin directory: {}", e), None))?;
    
    let dest = dest_dir.join(dest_name);
    fs::copy(src_path, &dest)
        .map_err(|e| InvalidData::new(&format!("Failed to copy {}: {}", src, e), None))?;
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dest)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&dest, perms)?;
    }
    
    Ok(())
}

/// Run make with proper flags
pub fn emake(env: &EbuildEnvironment, args: &[&str]) -> Result<Output, InvalidData> {
    let mut cmd = Command::new("make");
    
    // Add standard make flags
    if let Some(makeopts) = env.get("MAKEOPTS") {
        for opt in makeopts.split_whitespace() {
            cmd.arg(opt);
        }
    }
    
    // Add user-provided arguments
    for arg in args {
        cmd.arg(arg);
    }
    
    cmd.current_dir(&env.sourcedir)
        .output()
        .map_err(|e| InvalidData::new(&format!("Failed to run make: {}", e), None))
}

/// Default phase implementation
pub fn default(env: &mut EbuildEnvironment, phase: &str) -> Result<(), InvalidData> {
    match phase {
        "src_unpack" => default_src_unpack(env),
        "src_prepare" => default_src_prepare(env),
        "src_configure" => default_src_configure(env),
        "src_compile" => default_src_compile(env),
        "src_install" => default_src_install(env),
        _ => Ok(()),
    }
}

pub fn default_src_unpack(env: &EbuildEnvironment) -> Result<(), InvalidData> {
    use std::process::Command;

    // Get SRC_URI and A (archive files) from environment
    let distdir = env.get("DISTDIR").map(|s| s.clone()).unwrap_or_else(|| "/var/cache/distfiles".to_string());
    let a = env.get("A").map(|s| s.clone()).unwrap_or_default();
    let src_uri = env.get("SRC_URI").map(|s| s.clone()).unwrap_or_default();

    if a.is_empty() && src_uri.is_empty() {
        return Ok(()); // No sources to unpack
    }

    // Create distdir if it doesn't exist
    fs::create_dir_all(&distdir)
        .map_err(|e| InvalidData::new(&format!("Failed to create distdir: {}", e), None))?;

    // Create work directory
    fs::create_dir_all(&env.sourcedir)
        .map_err(|e| InvalidData::new(&format!("Failed to create source directory: {}", e), None))?;

    // Parse SRC_URI to get filename -> URIs mapping
    let uri_map = parse_src_uri(&src_uri);

    // Download and unpack each archive
    for archive in a.split_whitespace() {
        let archive_path = Path::new(&distdir).join(archive);

        // Try to download if not exists
        if !archive_path.exists() {
            einfo(&format!("Archive {} not found locally, attempting to download", archive));

            if let Some(uris) = uri_map.get(archive) {
                if !download_file(uris, &archive_path)? {
                    ewarn(&format!("Failed to download {}", archive));
                    continue;
                }
            } else {
                ewarn(&format!("No URI found for archive {}", archive));
                continue;
            }
        }

        einfo(&format!("Unpacking {}", archive));

        if archive.ends_with(".tar.gz") || archive.ends_with(".tgz") {
            unpack_tar(&archive_path, &env.sourcedir, Some("gzip"))?;
        } else if archive.ends_with(".tar.bz2") || archive.ends_with(".tbz2") {
            unpack_tar(&archive_path, &env.sourcedir, Some("bzip2"))?;
        } else if archive.ends_with(".tar.xz") || archive.ends_with(".txz") {
            unpack_tar(&archive_path, &env.sourcedir, Some("xz"))?;
        } else if archive.ends_with(".tar.zst") {
            unpack_tar(&archive_path, &env.sourcedir, Some("zstd"))?;
        } else if archive.ends_with(".tar") {
            unpack_tar(&archive_path, &env.sourcedir, None)?;
        } else if archive.ends_with(".zip") {
            unpack_zip(&archive_path, &env.sourcedir)?;
        } else {
            ewarn(&format!("Unknown archive format: {}", archive));
        }
    }

    Ok(())
}

/// Parse SRC_URI string into a mapping of filename -> list of URIs
fn parse_src_uri(src_uri: &str) -> HashMap<String, Vec<String>> {
    use std::collections::HashMap;

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
            if let Some(filename) = extract_filename_from_uri(&uri) {
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

/// Download a file from the given URIs, trying each one until successful
fn download_file(uris: &[String], dest_path: &Path) -> Result<bool, InvalidData> {
    for uri in uris {
        einfo(&format!("Trying to download from {}", uri));

        // Resolve mirror:// URIs
        let resolved_uri = if uri.starts_with("mirror://") {
            resolve_mirror_uri(uri)?
        } else {
            uri.clone()
        };

        // Try to download using curl
        if download_with_curl(&resolved_uri, dest_path)? {
            return Ok(true);
        }

        // Try wget as fallback
        if download_with_wget(&resolved_uri, dest_path)? {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Download file using curl
fn download_with_curl(uri: &str, dest_path: &Path) -> Result<bool, InvalidData> {
    use std::process::Command;

    let output = Command::new("curl")
        .arg("-L") // Follow redirects
        .arg("-o").arg(dest_path)
        .arg("--fail") // Fail on HTTP errors
        .arg("--silent")
        .arg("--show-error")
        .arg(uri)
        .output()
        .map_err(|e| InvalidData::new(&format!("Failed to run curl: {}", e), None))?;

    Ok(output.status.success())
}

/// Download file using wget
fn download_with_wget(uri: &str, dest_path: &Path) -> Result<bool, InvalidData> {
    use std::process::Command;

    let output = Command::new("wget")
        .arg("-O").arg(dest_path)
        .arg("--quiet")
        .arg(uri)
        .output()
        .map_err(|e| InvalidData::new(&format!("Failed to run wget: {}", e), None))?;

    Ok(output.status.success())
}

/// Resolve mirror:// URI to actual URLs
fn resolve_mirror_uri(uri: &str) -> Result<String, InvalidData> {
    // mirror://gentoo/distfiles/filename -> resolve using GENTOO_MIRRORS
    if uri.starts_with("mirror://gentoo/") {
        let path = &uri["mirror://gentoo/".len()..];

        // Get GENTOO_MIRRORS from environment
        if let Ok(mirrors) = std::env::var("GENTOO_MIRRORS") {
            for mirror in mirrors.split_whitespace() {
                let full_url = format!("{}/distfiles/{}", mirror.trim_end_matches('/'), path);
                return Ok(full_url);
            }
        }

        // Fallback to gentoo.org
        return Ok(format!("https://distfiles.gentoo.org/distfiles/{}", path));
    }

    // For other mirror types, just return as-is for now
    Ok(uri.to_string())
}

pub fn unpack_tar(archive: &Path, dest: &Path, compression: Option<&str>) -> Result<(), InvalidData> {
    use std::process::Command;
    
    let mut cmd = Command::new("tar");
    cmd.arg("-xf").arg(archive);
    cmd.arg("-C").arg(dest);
    
    if let Some(comp) = compression {
        let flag = match comp {
            "gzip" => "-z",
            "bzip2" => "-j",
            "xz" => "-J",
            "zstd" => "--zstd",
            _ => return Err(InvalidData::new(&format!("Unknown compression: {}", comp), None)),
        };
        cmd.arg(flag);
    }
    
    let output = cmd.output()
        .map_err(|e| InvalidData::new(&format!("Failed to run tar: {}", e), None))?;
    
    if !output.status.success() {
        return Err(InvalidData::new(&format!("Failed to unpack: {}", String::from_utf8_lossy(&output.stderr)), None));
    }
    
    Ok(())
}

pub fn unpack_zip(archive: &Path, dest: &Path) -> Result<(), InvalidData> {
    use std::process::Command;
    
    let output = Command::new("unzip")
        .arg("-q")
        .arg(archive)
        .arg("-d")
        .arg(dest)
        .output()
        .map_err(|e| InvalidData::new(&format!("Failed to run unzip: {}", e), None))?;
    
    if !output.status.success() {
        return Err(InvalidData::new(&format!("Failed to unpack: {}", String::from_utf8_lossy(&output.stderr)), None));
    }
    
    Ok(())
}

pub fn default_src_prepare(env: &EbuildEnvironment) -> Result<(), InvalidData> {
    // Apply patches from PATCHES array or default locations
    let patches_dir = env.sourcedir.join("patches");
    
    if patches_dir.exists() {
        einfo("Applying patches from patches/ directory");
        apply_patches(&patches_dir, &env.sourcedir)?;
    }
    
    // Run eapply_user if EAPI >= 6
    if env.eapi.parse::<u32>().unwrap_or(0) >= 6 {
        eapply_user(env)?;
    }
    
    Ok(())
}

fn apply_patches(patches_dir: &Path, source_dir: &Path) -> Result<(), InvalidData> {
    use std::process::Command;
    
    let mut patches: Vec<PathBuf> = fs::read_dir(patches_dir)
        .map_err(|e| InvalidData::new(&format!("Failed to read patches directory: {}", e), None))?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().map_or(false, |ext| ext == "patch" || ext == "diff"))
        .collect();
    
    patches.sort();
    
    for patch in patches {
        einfo(&format!("Applying {}", patch.file_name().unwrap().to_string_lossy()));
        
        let output = Command::new("patch")
            .arg("-p1")
            .arg("-i")
            .arg(&patch)
            .current_dir(source_dir)
            .output()
            .map_err(|e| InvalidData::new(&format!("Failed to run patch: {}", e), None))?;
        
        if !output.status.success() {
            return Err(InvalidData::new(&format!("Failed to apply patch: {}", String::from_utf8_lossy(&output.stderr)), None));
        }
    }
    
    Ok(())
}

fn eapply_user(env: &EbuildEnvironment) -> Result<(), InvalidData> {
    // Check for user patches in /etc/portage/patches/
    let category = env.get("CATEGORY").map(|s| s.clone()).unwrap_or_default();
    let pf = env.get("PF").map(|s| s.clone()).unwrap_or_default();
    let p = env.get("P").map(|s| s.clone()).unwrap_or_default();
    
    if category.is_empty() || pf.is_empty() {
        return Ok(());
    }
    
    let patch_dirs = vec![
        format!("/etc/portage/patches/{}/{}", category, pf),
        format!("/etc/portage/patches/{}/{}", category, p),
        format!("/etc/portage/patches/{}", category),
    ];
    
    for patch_dir in patch_dirs {
        let dir_path = Path::new(&patch_dir);
        if dir_path.exists() && dir_path.is_dir() {
            einfo(&format!("Applying user patches from {}", patch_dir));
            
            let mut patches: Vec<_> = fs::read_dir(dir_path)
                .map_err(|e| InvalidData::new(&format!("Failed to read patch dir: {}", e), None))?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let path = e.path();
                    path.is_file() && (
                        path.extension().map_or(false, |ext| ext == "patch" || ext == "diff")
                    )
                })
                .collect();
            
            patches.sort_by_key(|e| e.path());
            
            for patch in patches {
                let patch_path = patch.path();
                einfo(&format!("Applying {}", patch_path.file_name().unwrap().to_string_lossy()));
                
                let output = std::process::Command::new("patch")
                    .arg("-p1")
                    .arg("-i")
                    .arg(&patch_path)
                    .current_dir(&env.sourcedir)
                    .output()
                    .map_err(|e| InvalidData::new(&format!("Failed to run patch: {}", e), None))?;
                
                if !output.status.success() {
                    return Err(InvalidData::new(
                        &format!("Failed to apply patch {}: {}", 
                            patch_path.display(),
                            String::from_utf8_lossy(&output.stderr)
                        ), 
                        None
                    ));
                }
            }
            
            return Ok(()); // Only apply from first matching directory
        }
    }
    
    Ok(())
}

pub fn default_src_configure(env: &mut EbuildEnvironment) -> Result<(), InvalidData> {
    use std::process::Command;
    
    let configure_script = env.sourcedir.join("configure");
    let cmake_file = env.sourcedir.join("CMakeLists.txt");
    let meson_file = env.sourcedir.join("meson.build");
    
    if configure_script.exists() {
        einfo("Running ./configure");
        econf(env, &[])?;
    } else if cmake_file.exists() {
        einfo("Detected CMake build system");
        // CMake configuration is handled by cmake eclass
        super::eclass::cmake::src_configure(env)?;
    } else if meson_file.exists() {
        einfo("Detected Meson build system");
        // Meson configuration is handled by meson eclass
        super::eclass::meson::src_configure(env, &[])?;
    } else {
        einfo("No configure script or build system detected");
    }
    
    Ok(())
}

/// Run configure script with standard options
pub fn econf(env: &EbuildEnvironment, extra_args: &[&str]) -> Result<(), InvalidData> {
    use std::process::Command;
    
    let configure_script = env.sourcedir.join("configure");
    if !configure_script.exists() {
        return Err(InvalidData::new("configure script not found", None));
    }
    
    let mut cmd = Command::new("./configure");
    
    // Add standard configure options
    cmd.arg(format!("--prefix={}/usr", env.destdir.display()));
    cmd.arg(format!("--sysconfdir={}/etc", env.destdir.display()));
    cmd.arg(format!("--localstatedir={}/var", env.destdir.display()));
    
    // Add library directory
    if let Some(libdir) = env.get("LIBDIR") {
        cmd.arg(format!("--libdir={}/usr/{}", env.destdir.display(), libdir));
    } else {
        cmd.arg(format!("--libdir={}/usr/lib", env.destdir.display()));
    }
    
    // Add extra arguments
    for arg in extra_args {
        cmd.arg(arg);
    }
    
    cmd.current_dir(&env.sourcedir);
    
    let output = cmd.output()
        .map_err(|e| InvalidData::new(&format!("Failed to run configure: {}", e), None))?;
    
    if !output.status.success() {
        return Err(InvalidData::new(&format!("configure failed: {}", String::from_utf8_lossy(&output.stderr)), None));
    }
    
    Ok(())
}

pub fn default_src_compile(env: &EbuildEnvironment) -> Result<(), InvalidData> {
    emake(env, &[])?;
    Ok(())
}

pub fn default_src_install(env: &EbuildEnvironment) -> Result<(), InvalidData> {
    let destdir = env.destdir.to_string_lossy().to_string();
    let output = emake(env, &["install", &format!("DESTDIR={}", destdir)])?;
    
    if !output.status.success() {
        return Err(InvalidData::new("make install failed", None));
    }
    
    Ok(())
}
