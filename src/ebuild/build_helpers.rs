// build_helpers.rs - Build system helper functions
use std::process::{Command, Output};
use std::path::Path;
use crate::exception::InvalidData;
use super::environment::EbuildEnvironment;
use super::portage_helpers::*;

/// Run make with proper flags
pub fn emake(env: &EbuildEnvironment, args: &[&str]) -> Result<Output, InvalidData> {
    let mut cmd = Command::new("make");

    // Set environment variables from the ebuild environment
    for (key, value) in &env.vars {
        cmd.env(key, value);
    }

    // Add MAKEOPTS
    if let Some(makeopts) = env.get("MAKEOPTS") {
        for opt in makeopts.split_whitespace() {
            cmd.arg(opt);
        }
    } else {
        // Default to parallel build
        let jobs = num_cpus::get();
        cmd.arg(format!("-j{}", jobs));
    }

    // Add user arguments
    for arg in args {
        cmd.arg(arg);
    }

    cmd.current_dir(&env.sourcedir)
        .output()
        .map_err(|e| InvalidData::new(&format!("Failed to run make: {}", e), None))
}

/// Configure with econf
pub fn econf(env: &EbuildEnvironment, extra_args: &[&str]) -> Result<(), InvalidData> {
    let configure_script = env.sourcedir.join("configure");
    if !configure_script.exists() {
        return Err(InvalidData::new("configure script not found", None));
    }

    let mut cmd = Command::new("./configure");

    // Set environment variables from the ebuild environment
    for (key, value) in &env.vars {
        cmd.env(key, value);
    }
    
    // Standard GNU configure options
    cmd.arg(format!("--prefix=/usr"));
    cmd.arg(format!("--build={}", get_build_tuple(env)));
    cmd.arg(format!("--host={}", get_host_tuple(env)));
    cmd.arg(format!("--mandir=/usr/share/man"));
    cmd.arg(format!("--infodir=/usr/share/info"));
    cmd.arg(format!("--datadir=/usr/share"));
    cmd.arg(format!("--sysconfdir=/etc"));
    cmd.arg(format!("--localstatedir=/var/lib"));
    
    // Library directory
    let libdir = env.get("LIBDIR").unwrap_or(&"lib".to_string()).clone();
    cmd.arg(format!("--libdir=/usr/{}", libdir));
    
    // Disable dependency tracking for faster builds
    cmd.arg("--disable-dependency-tracking");
    
    // EAPI 7+: --datarootdir
    if env.eapi.parse::<u32>().unwrap_or(0) >= 7 {
        cmd.arg("--datarootdir=/usr/share");
    }
    
    // Add extra arguments
    for arg in extra_args {
        cmd.arg(arg);
    }
    
    cmd.current_dir(&env.sourcedir);
    
    einfo("Configuring with econf");
    let output = cmd.output()
        .map_err(|e| InvalidData::new(&format!("Failed to run configure: {}", e), None))?;
    
    if !output.status.success() {
        eerror(&format!("configure failed:\n{}", String::from_utf8_lossy(&output.stderr)));
        return Err(InvalidData::new("configure failed", None));
    }
    
    Ok(())
}

/// Install with einstall (deprecated, prefer emake install DESTDIR=...)
pub fn einstall(env: &EbuildEnvironment, extra_args: &[&str]) -> Result<(), InvalidData> {
    let mut cmd = Command::new("make");
    cmd.arg("install");
    
    // Add standard install directories
    cmd.arg(format!("prefix={}/usr", env.destdir.display()));
    cmd.arg(format!("datadir={}/usr/share", env.destdir.display()));
    cmd.arg(format!("infodir={}/usr/share/info", env.destdir.display()));
    cmd.arg(format!("localstatedir={}/var/lib", env.destdir.display()));
    cmd.arg(format!("mandir={}/usr/share/man", env.destdir.display()));
    cmd.arg(format!("sysconfdir={}/etc", env.destdir.display()));
    
    let libdir = env.get("LIBDIR").unwrap_or(&"lib".to_string()).clone();
    cmd.arg(format!("libdir={}/usr/{}", env.destdir.display(), libdir));
    
    // Add extra arguments
    for arg in extra_args {
        cmd.arg(arg);
    }
    
    cmd.current_dir(&env.sourcedir);
    
    let output = cmd.output()
        .map_err(|e| InvalidData::new(&format!("Failed to run make install: {}", e), None))?;
    
    if !output.status.success() {
        return Err(InvalidData::new("make install failed", None));
    }
    
    Ok(())
}

/// Apply patches
pub fn epatch(env: &EbuildEnvironment, patches: &[&str]) -> Result<(), InvalidData> {
    for patch in patches {
        let patch_path = Path::new(patch);
        if !patch_path.exists() {
            return Err(InvalidData::new(&format!("Patch not found: {}", patch), None));
        }
        
        einfo(&format!("Applying {}", patch));
        
        let output = Command::new("patch")
            .arg("-p1")
            .arg("-i")
            .arg(patch_path)
            .current_dir(&env.sourcedir)
            .output()
            .map_err(|e| InvalidData::new(&format!("Failed to run patch: {}", e), None))?;
        
        if !output.status.success() {
            return Err(InvalidData::new(&format!("Failed to apply patch: {}", String::from_utf8_lossy(&output.stderr)), None));
        }
    }
    
    Ok(())
}

/// Apply patches with eapply (EAPI 6+)
pub fn eapply(env: &EbuildEnvironment, patches: &[&str]) -> Result<(), InvalidData> {
    epatch(env, patches)
}

/// Apply user patches (EAPI 6+)
pub fn eapply_user(env: &EbuildEnvironment) -> Result<(), InvalidData> {
    // Check /etc/portage/patches/
    let category = env.get("CATEGORY").map(|s| s.clone()).unwrap_or_default();
    let pf = env.get("PF").map(|s| s.clone()).unwrap_or_default();
    let p = env.get("P").map(|s| s.clone()).unwrap_or_default();
    
    let patch_dirs = vec![
        format!("/etc/portage/patches/{}/{}", category, pf),
        format!("/etc/portage/patches/{}/{}", category, p),
        format!("/etc/portage/patches/{}", category),
    ];
    
    for patch_dir in patch_dirs {
        let dir_path = Path::new(&patch_dir);
        if dir_path.exists() && dir_path.is_dir() {
            einfo(&format!("Applying user patches from {}", patch_dir));
            
            let mut patches: Vec<_> = std::fs::read_dir(dir_path)
                .map_err(|e| InvalidData::new(&format!("Failed to read patch dir: {}", e), None))?
                .filter_map(|e| e.ok())
                .filter(|e| {
                    let path = e.path();
                    path.is_file() && (path.extension().map_or(false, |ext| ext == "patch" || ext == "diff"))
                })
                .collect();
            
            patches.sort_by_key(|e| e.path());
            
            for patch in patches {
                let patch_str = patch.path().to_string_lossy().to_string();
                epatch(env, &[&patch_str])?;
            }
            
            break; // Only apply from first matching directory
        }
    }
    
    Ok(())
}

/// Run autoreconf
pub fn eautoreconf(env: &EbuildEnvironment) -> Result<(), InvalidData> {
    einfo("Running autoreconf");
    
    let output = Command::new("autoreconf")
        .arg("-f")
        .arg("-i")
        .current_dir(&env.sourcedir)
        .output()
        .map_err(|e| InvalidData::new(&format!("Failed to run autoreconf: {}", e), None))?;
    
    if !output.status.success() {
        return Err(InvalidData::new("autoreconf failed", None));
    }
    
    Ok(())
}

/// Run libtoolize
pub fn elibtoolize(env: &EbuildEnvironment) -> Result<(), InvalidData> {
    einfo("Running libtoolize");
    
    let output = Command::new("libtoolize")
        .arg("--copy")
        .arg("--force")
        .current_dir(&env.sourcedir)
        .output()
        .map_err(|e| InvalidData::new(&format!("Failed to run libtoolize: {}", e), None))?;
    
    if !output.status.success() {
        return Err(InvalidData::new("libtoolize failed", None));
    }
    
    Ok(())
}

/// Run aclocal
pub fn eaclocal(env: &EbuildEnvironment) -> Result<(), InvalidData> {
    let output = Command::new("aclocal")
        .current_dir(&env.sourcedir)
        .output()
        .map_err(|e| InvalidData::new(&format!("Failed to run aclocal: {}", e), None))?;
    
    if !output.status.success() {
        return Err(InvalidData::new("aclocal failed", None));
    }
    
    Ok(())
}

/// Run autoconf
pub fn eautoconf(env: &EbuildEnvironment) -> Result<(), InvalidData> {
    let output = Command::new("autoconf")
        .current_dir(&env.sourcedir)
        .output()
        .map_err(|e| InvalidData::new(&format!("Failed to run autoconf: {}", e), None))?;
    
    if !output.status.success() {
        return Err(InvalidData::new("autoconf failed", None));
    }
    
    Ok(())
}

/// Run automake
pub fn eautomake(env: &EbuildEnvironment) -> Result<(), InvalidData> {
    let output = Command::new("automake")
        .arg("--add-missing")
        .arg("--copy")
        .current_dir(&env.sourcedir)
        .output()
        .map_err(|e| InvalidData::new(&format!("Failed to run automake: {}", e), None))?;
    
    if !output.status.success() {
        return Err(InvalidData::new("automake failed", None));
    }
    
    Ok(())
}

/// Get build tuple (e.g., x86_64-pc-linux-gnu)
fn get_build_tuple(env: &EbuildEnvironment) -> String {
    env.get("CBUILD")
        .or_else(|| env.get("CHOST"))
        .cloned()
        .unwrap_or_else(|| {
            // Try to detect from system
            std::process::Command::new("gcc")
                .arg("-dumpmachine")
                .output()
                .ok()
                .and_then(|o| String::from_utf8(o.stdout).ok())
                .map(|s| s.trim().to_string())
                .unwrap_or_else(|| "x86_64-pc-linux-gnu".to_string())
        })
}

/// Get host tuple
fn get_host_tuple(env: &EbuildEnvironment) -> String {
    env.get("CHOST")
        .cloned()
        .unwrap_or_else(|| get_build_tuple(env))
}

/// Strip binaries in D
pub fn prepstrip(env: &EbuildEnvironment) -> Result<(), InvalidData> {
    if env.get("RESTRICT").map_or(false, |r| r.contains("strip")) {
        return Ok(());
    }
    
    einfo("Stripping binaries");
    
    // Find and strip binaries
    find_and_strip(&env.destdir.join("usr/bin"))?;
    find_and_strip(&env.destdir.join("usr/sbin"))?;
    find_and_strip(&env.destdir.join("bin"))?;
    find_and_strip(&env.destdir.join("sbin"))?;
    
    // Find and strip libraries
    let libdir = env.get("LIBDIR").unwrap_or(&"lib".to_string()).clone();
    find_and_strip(&env.destdir.join("usr").join(&libdir))?;
    find_and_strip(&env.destdir.join(&libdir))?;
    
    Ok(())
}

fn find_and_strip(dir: &Path) -> Result<(), InvalidData> {
    if !dir.exists() {
        return Ok(());
    }
    
    for entry in std::fs::read_dir(dir)
        .map_err(|e| InvalidData::new(&format!("Failed to read dir: {}", e), None))? 
    {
        let entry = entry.map_err(|e| InvalidData::new(&format!("Failed to read entry: {}", e), None))?;
        let path = entry.path();
        
        if path.is_file() && is_elf_binary(&path) {
            Command::new("strip")
                .arg("--strip-unneeded")
                .arg(&path)
                .output()
                .ok();
        }
    }
    
    Ok(())
}

fn is_elf_binary(path: &Path) -> bool {
    use std::fs::File;
    use std::io::Read;
    
    if let Ok(mut file) = File::open(path) {
        let mut magic = [0u8; 4];
        if file.read_exact(&mut magic).is_ok() {
            return magic == [0x7f, b'E', b'L', b'F'];
        }
    }
    false
}
