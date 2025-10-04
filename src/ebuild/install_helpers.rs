// install_helpers.rs - Installation helper functions (dobin, doins, etc.)
use std::fs;
use std::path::{Path, PathBuf};
use crate::exception::InvalidData;
use super::environment::EbuildEnvironment;
use super::portage_helpers::*;

/// Install executables into /usr/bin
pub fn dobin(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    into_helper(env, "bin", files, 0o755, "/usr/bin")
}

/// Install executables into /usr/sbin
pub fn dosbin(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    into_helper(env, "sbin", files, 0o755, "/usr/sbin")
}

/// Install libraries into /usr/lib
pub fn dolib(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    let libdir = env.get("LIBDIR").map(|s| s.as_str()).unwrap_or("lib");
    into_helper(env, "lib", files, 0o644, &format!("/usr/{}", libdir))
}

/// Install shared libraries
pub fn dolib_so(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    let libdir = env.get("LIBDIR").map(|s| s.as_str()).unwrap_or("lib");
    into_helper(env, "lib.so", files, 0o755, &format!("/usr/{}", libdir))
}

/// Install static libraries
pub fn dolib_a(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    let libdir = env.get("LIBDIR").map(|s| s.as_str()).unwrap_or("lib");
    into_helper(env, "lib.a", files, 0o644, &format!("/usr/{}", libdir))
}

/// Install data files
pub fn doins(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    let insinto = env.get("INSINTO").map(|s| s.clone()).unwrap_or_else(|| "/usr/share".to_string());
    into_helper(env, "ins", files, 0o644, &insinto)
}

/// Install documentation
pub fn dodoc(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    let pf = env.get("PF").map(|s| s.as_str()).unwrap_or("unknown");
    let docdir = format!("/usr/share/doc/{}", pf);
    into_helper(env, "doc", files, 0o644, &docdir)
}

/// Install HTML documentation
pub fn dohtml(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    let pf = env.get("PF").map(|s| s.as_str()).unwrap_or("unknown");
    let htmldir = format!("/usr/share/doc/{}/html", pf);
    into_helper(env, "html", files, 0o644, &htmldir)
}

/// Install man pages
pub fn doman(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    for file in files {
        let src = Path::new(file);
        if !src.exists() {
            return Err(InvalidData::new(&format!("doman: {} not found", file), None));
        }
        
        let filename = src.file_name()
            .ok_or_else(|| InvalidData::new(&format!("Invalid filename: {}", file), None))?
            .to_string_lossy();
        
        // Determine man section from filename
        let section = if let Some(ext) = filename.split('.').last() {
            if ext.chars().all(|c| c.is_ascii_digit()) {
                ext
            } else {
                return Err(InvalidData::new(&format!("Cannot determine man section for {}", file), None));
            }
        } else {
            return Err(InvalidData::new(&format!("Cannot determine man section for {}", file), None));
        };
        
        let mandir = format!("/usr/share/man/man{}", section);
        into_helper(env, "man", &[file], 0o644, &mandir)?;
    }
    
    Ok(())
}

/// Install info pages
pub fn doinfo(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    into_helper(env, "info", files, 0o644, "/usr/share/info")
}

/// Install init scripts
pub fn doinitd(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    into_helper(env, "initd", files, 0o755, "/etc/init.d")
}

/// Install conf.d files
pub fn doconfd(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    into_helper(env, "confd", files, 0o644, "/etc/conf.d")
}

/// Install env.d files
pub fn doenvd(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    into_helper(env, "envd", files, 0o644, "/etc/env.d")
}

/// Install with new name
pub fn newbin(env: &EbuildEnvironment, src: &str, dest_name: &str) -> Result<(), InvalidData> {
    new_helper(env, "bin", src, dest_name, 0o755, "/usr/bin")
}

/// Install sbin with new name
pub fn newsbin(env: &EbuildEnvironment, src: &str, dest_name: &str) -> Result<(), InvalidData> {
    new_helper(env, "sbin", src, dest_name, 0o755, "/usr/sbin")
}

/// Install lib with new name
pub fn newlib(env: &EbuildEnvironment, src: &str, dest_name: &str) -> Result<(), InvalidData> {
    let libdir = env.get("LIBDIR").map(|s| s.as_str()).unwrap_or("lib");
    new_helper(env, "lib", src, dest_name, 0o644, &format!("/usr/{}", libdir))
}

/// Install file with new name
pub fn newins(env: &EbuildEnvironment, src: &str, dest_name: &str) -> Result<(), InvalidData> {
    let insinto = env.get("INSINTO").map(|s| s.clone()).unwrap_or_else(|| "/usr/share".to_string());
    new_helper(env, "ins", src, dest_name, 0o644, &insinto)
}

/// Install doc with new name
pub fn newdoc(env: &EbuildEnvironment, src: &str, dest_name: &str) -> Result<(), InvalidData> {
    let pf = env.get("PF").map(|s| s.as_str()).unwrap_or("unknown");
    let docdir = format!("/usr/share/doc/{}", pf);
    new_helper(env, "doc", src, dest_name, 0o644, &docdir)
}

/// Install man with new name
pub fn newman(env: &EbuildEnvironment, src: &str, dest_name: &str) -> Result<(), InvalidData> {
    let section = dest_name.split('.').last()
        .ok_or_else(|| InvalidData::new("Cannot determine man section", None))?;
    
    let mandir = format!("/usr/share/man/man{}", section);
    new_helper(env, "man", src, dest_name, 0o644, &mandir)
}

/// Install init.d with new name
pub fn newinitd(env: &EbuildEnvironment, src: &str, dest_name: &str) -> Result<(), InvalidData> {
    new_helper(env, "initd", src, dest_name, 0o755, "/etc/init.d")
}

/// Install conf.d with new name
pub fn newconfd(env: &EbuildEnvironment, src: &str, dest_name: &str) -> Result<(), InvalidData> {
    new_helper(env, "confd", src, dest_name, 0o644, "/etc/conf.d")
}

/// Install env.d with new name
pub fn newenvd(env: &EbuildEnvironment, src: &str, dest_name: &str) -> Result<(), InvalidData> {
    new_helper(env, "envd", src, dest_name, 0o644, "/etc/env.d")
}

/// Set installation directory for doins
pub fn insinto(env: &mut EbuildEnvironment, dir: &str) {
    env.set("INSINTO".to_string(), dir.to_string());
}

/// Set installation directory for exeinto
pub fn exeinto(env: &mut EbuildEnvironment, dir: &str) {
    env.set("EXEINTO".to_string(), dir.to_string());
}

/// Set installation directory for docinto
pub fn docinto(env: &mut EbuildEnvironment, dir: &str) {
    let pf = env.get("PF").map(|s| s.clone()).unwrap_or_else(|| "unknown".to_string());
    let docdir = if dir == "/" {
        format!("/usr/share/doc/{}", pf)
    } else {
        format!("/usr/share/doc/{}/{}", pf, dir.trim_start_matches('/'))
    };
    env.set("DOCINTO".to_string(), docdir);
}

/// Install executables (uses EXEINTO)
pub fn doexe(env: &EbuildEnvironment, files: &[&str]) -> Result<(), InvalidData> {
    let exeinto = env.get("EXEINTO").map(|s| s.clone()).unwrap_or_else(|| "/usr/bin".to_string());
    into_helper(env, "exe", files, 0o755, &exeinto)
}

/// Install executable with new name
pub fn newexe(env: &EbuildEnvironment, src: &str, dest_name: &str) -> Result<(), InvalidData> {
    let exeinto = env.get("EXEINTO").map(|s| s.clone()).unwrap_or_else(|| "/usr/bin".to_string());
    new_helper(env, "exe", src, dest_name, 0o755, &exeinto)
}

/// Create directory
pub fn dodir(env: &EbuildEnvironment, dirs: &[&str]) -> Result<(), InvalidData> {
    for dir in dirs {
        let dest = env.destdir.join(dir.trim_start_matches('/'));
        fs::create_dir_all(&dest)
            .map_err(|e| InvalidData::new(&format!("dodir: Failed to create {}: {}", dir, e), None))?;
    }
    Ok(())
}

/// Create directory and keep it (even if empty)
pub fn keepdir(env: &EbuildEnvironment, dirs: &[&str]) -> Result<(), InvalidData> {
    for dir in dirs {
        let dest = env.destdir.join(dir.trim_start_matches('/'));
        fs::create_dir_all(&dest)
            .map_err(|e| InvalidData::new(&format!("keepdir: Failed to create {}: {}", dir, e), None))?;
        
        // Create .keep file
        let keep_file = dest.join(".keep_portage");
        fs::write(&keep_file, "")
            .map_err(|e| InvalidData::new(&format!("keepdir: Failed to create .keep file: {}", e), None))?;
    }
    Ok(())
}

/// Create symbolic link
pub fn dosym(env: &EbuildEnvironment, target: &str, link: &str) -> Result<(), InvalidData> {
    let link_path = env.destdir.join(link.trim_start_matches('/'));
    
    if let Some(parent) = link_path.parent() {
        fs::create_dir_all(parent)
            .map_err(|e| InvalidData::new(&format!("dosym: Failed to create parent dir: {}", e), None))?;
    }
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        symlink(target, &link_path)
            .map_err(|e| InvalidData::new(&format!("dosym: Failed to create symlink: {}", e), None))?;
    }
    
    #[cfg(not(unix))]
    {
        return Err(InvalidData::new("dosym: Symbolic links not supported on this platform", None));
    }
    
    Ok(())
}

/// Helper function for installing files
fn into_helper(env: &EbuildEnvironment, helper: &str, files: &[&str], mode: u32, dest_base: &str) -> Result<(), InvalidData> {
    let dest_dir = env.destdir.join(dest_base.trim_start_matches('/'));
    
    fs::create_dir_all(&dest_dir)
        .map_err(|e| InvalidData::new(&format!("do{}: Failed to create directory: {}", helper, e), None))?;
    
    for file in files {
        let src = Path::new(file);
        if !src.exists() {
            return Err(InvalidData::new(&format!("do{}: {} not found", helper, file), None));
        }
        
        let filename = src.file_name()
            .ok_or_else(|| InvalidData::new(&format!("Invalid filename: {}", file), None))?;
        let dest = dest_dir.join(filename);
        
        fs::copy(src, &dest)
            .map_err(|e| InvalidData::new(&format!("do{}: Failed to copy {}: {}", helper, file, e), None))?;
        
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&dest)
                .map_err(|e| InvalidData::new(&format!("Failed to get metadata: {}", e), None))?
                .permissions();
            perms.set_mode(mode);
            fs::set_permissions(&dest, perms)
                .map_err(|e| InvalidData::new(&format!("Failed to set permissions: {}", e), None))?;
        }
    }
    
    Ok(())
}

/// Helper function for installing with new name
fn new_helper(env: &EbuildEnvironment, helper: &str, src: &str, dest_name: &str, mode: u32, dest_base: &str) -> Result<(), InvalidData> {
    let src_path = Path::new(src);
    if !src_path.exists() {
        return Err(InvalidData::new(&format!("new{}: {} not found", helper, src), None));
    }
    
    let dest_dir = env.destdir.join(dest_base.trim_start_matches('/'));
    fs::create_dir_all(&dest_dir)
        .map_err(|e| InvalidData::new(&format!("new{}: Failed to create directory: {}", helper, e), None))?;
    
    let dest = dest_dir.join(dest_name);
    fs::copy(src_path, &dest)
        .map_err(|e| InvalidData::new(&format!("new{}: Failed to copy {}: {}", helper, src, e), None))?;
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&dest)
            .map_err(|e| InvalidData::new(&format!("Failed to get metadata: {}", e), None))?
            .permissions();
        perms.set_mode(mode);
        fs::set_permissions(&dest, perms)
            .map_err(|e| InvalidData::new(&format!("Failed to set permissions: {}", e), None))?;
    }
    
    Ok(())
}

/// Compress man pages
pub fn doman_compress(env: &EbuildEnvironment) -> Result<(), InvalidData> {
    use std::process::Command;
    
    let man_dir = env.destdir.join("usr/share/man");
    if !man_dir.exists() {
        return Ok(());
    }
    
    for entry in fs::read_dir(&man_dir)
        .map_err(|e| InvalidData::new(&format!("Failed to read man dir: {}", e), None))? 
    {
        let entry = entry.map_err(|e| InvalidData::new(&format!("Failed to read entry: {}", e), None))?;
        let path = entry.path();
        
        if path.is_dir() {
            for man_file in fs::read_dir(&path)
                .map_err(|e| InvalidData::new(&format!("Failed to read section: {}", e), None))? 
            {
                let man_file = man_file.map_err(|e| InvalidData::new(&format!("Failed to read file: {}", e), None))?;
                let file_path = man_file.path();
                
                if file_path.is_file() && !file_path.to_string_lossy().ends_with(".gz") {
                    Command::new("gzip")
                        .arg("-9")
                        .arg(&file_path)
                        .output()
                        .map_err(|e| InvalidData::new(&format!("Failed to compress {}: {}", file_path.display(), e), None))?;
                }
            }
        }
    }
    
    Ok(())
}
