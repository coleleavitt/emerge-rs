use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::process::Command;
use crate::exception::InvalidData;
use super::helpers::{einfo, ewarn};

pub struct Downloader {
    distdir: String,
    mirrors: HashMap<String, Vec<String>>,
    gentoo_mirrors: Vec<String>,
}

impl Downloader {
    pub fn new(distdir: &str) -> Self {
        let mut mirrors = HashMap::new();
        
        mirrors.insert("debian".to_string(), vec![
            "https://deb.debian.org/debian/".to_string(),
            "http://ftp.us.debian.org/debian/".to_string(),
        ]);
        
        mirrors.insert("gentoo".to_string(), vec![
            "https://distfiles.gentoo.org/distfiles".to_string(),
            "https://gentoo.osuosl.org/distfiles".to_string(),
        ]);
        
        mirrors.insert("gnu".to_string(), vec![
            "https://ftp.gnu.org/gnu/".to_string(),
            "https://www.mirrorservice.org/sites/ftp.gnu.org/gnu/".to_string(),
        ]);
        
        mirrors.insert("apache".to_string(), vec![
            "https://dlcdn.apache.org/".to_string(),
            "https://archive.apache.org/dist/".to_string(),
        ]);
        
        let gentoo_mirrors = vec![
            "https://mirrors.rit.edu/gentoo/".to_string(),
            "https://gentoo.osuosl.org/".to_string(),
        ];
        
        Self {
            distdir: distdir.to_string(),
            mirrors,
            gentoo_mirrors,
        }
    }
    
    pub fn load_thirdparty_mirrors(&mut self, mirrors_file: &Path) -> Result<(), InvalidData> {
        if !mirrors_file.exists() {
            return Ok(());
        }
        
        let content = fs::read_to_string(mirrors_file)
            .map_err(|e| InvalidData::new(&format!("Failed to read mirrors file: {}", e), None))?;
        
        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 2 {
                let mirror_name = parts[0].to_string();
                let urls: Vec<String> = parts[1..].iter().map(|s| s.to_string()).collect();
                self.mirrors.insert(mirror_name, urls);
            }
        }
        
        Ok(())
    }
    
    pub fn download(&self, uri: &str, filename: &str) -> Result<(), InvalidData> {
        let dest_path = Path::new(&self.distdir).join(filename);
        
        if dest_path.exists() {
            einfo(&format!("Using cached {}", filename));
            return Ok(());
        }
        
        fs::create_dir_all(&self.distdir)
            .map_err(|e| InvalidData::new(&format!("Failed to create DISTDIR: {}", e), None))?;
        
        let urls = self.expand_mirror_uri(uri);
        
        for url in &urls {
            einfo(&format!("Downloading {} from {}", filename, url));
            
            if self.try_download(url, &dest_path)? {
                einfo(&format!("Successfully downloaded {}", filename));
                return Ok(());
            }
            
            ewarn(&format!("Failed to download from {}", url));
        }
        
        Err(InvalidData::new(&format!("Failed to download {} from all mirrors", filename), None))
    }
    
    fn expand_mirror_uri(&self, uri: &str) -> Vec<String> {
        if uri.starts_with("mirror://") {
            let rest = &uri[9..];
            if let Some(slash_pos) = rest.find('/') {
                let mirror_type = &rest[..slash_pos];
                let path = &rest[slash_pos + 1..];
                
                if let Some(mirror_urls) = self.mirrors.get(mirror_type) {
                    return mirror_urls.iter()
                        .map(|base| format!("{}{}", base, path))
                        .collect();
                }
            }
            
            vec![uri.to_string()]
        } else if uri.starts_with("http://") || uri.starts_with("https://") || uri.starts_with("ftp://") {
            vec![uri.to_string()]
        } else {
            self.gentoo_mirrors.iter()
                .map(|base| format!("{}{}", base, uri))
                .collect()
        }
    }
    
    fn try_download(&self, url: &str, dest: &Path) -> Result<bool, InvalidData> {
        let temp_dest = dest.with_extension("tmp");
        
        let status = Command::new("wget")
            .args(&[
                "--quiet",
                "--tries=1",
                "--timeout=60",
                "--output-document",
                temp_dest.to_str().unwrap(),
                url
            ])
            .status();
        
        match status {
            Ok(s) if s.success() => {
                fs::rename(&temp_dest, dest)
                    .map_err(|e| InvalidData::new(&format!("Failed to move downloaded file: {}", e), None))?;
                Ok(true)
            }
            _ => {
                let _ = fs::remove_file(&temp_dest);
                Ok(false)
            }
        }
    }
}
