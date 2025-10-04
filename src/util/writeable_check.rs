// writeable_check.rs -- Check for read-only filesystems

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::MetadataExt;
use std::path::Path;

pub fn get_ro_checker() -> fn(Vec<&Path>) -> Vec<String> {
    // For simplicity, assume Linux
    linux_ro_checker
}

pub fn linux_ro_checker(dir_list: Vec<&Path>) -> Vec<String> {
    let mut ro_filesystems = HashSet::new();

    let content = match fs::read_to_string("/proc/self/mountinfo") {
        Ok(c) => c,
        Err(_) => return vec![],
    };

    for line in content.lines() {
        let mount: Vec<&str> = line.split(" - ").collect();
        if mount.len() < 2 {
            continue;
        }
        let parts: Vec<&str> = mount[0].split_whitespace().collect();
        if parts.len() < 6 {
            continue;
        }
        let _dir = parts[4];
        let attr1 = parts[5];
        let mount_parts: Vec<&str> = mount[1].split_whitespace().collect();
        let attr2 = if mount_parts.len() >= 3 {
            mount_parts[2]
        } else if mount_parts.len() >= 2 {
            mount_parts[1]
        } else {
            continue;
        };
        if attr1.starts_with("ro") || attr2.starts_with("ro") {
            ro_filesystems.insert(_dir.to_string());
        }
    }

    let mut ro_devs = std::collections::HashMap::new();
    for x in &ro_filesystems {
        if let Ok(meta) = fs::metadata(x) {
            ro_devs.insert(meta.dev(), x.clone());
        }
    }

    let mut result = HashSet::new();
    for dir in dir_list {
        if let Ok(meta) = fs::metadata(dir) {
            if let Some(fs) = ro_devs.get(&meta.dev()) {
                result.insert(fs.clone());
            }
        }
    }

    result.into_iter().collect()
}

pub fn empty_ro_checker(_dir_list: Vec<&Path>) -> Vec<String> {
    vec![]
}