// bintree.rs -- Binary package database (/usr/portage/packages)

use std::collections::HashMap;
use tokio::fs;
use tokio::io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt};
use std::path::Path;
use crate::exception::InvalidData;
use crate::xpak;

#[derive(Debug)]
pub struct BinTree {
    pub root: String,
    pub pkgdir: String,
    pub binhost: Vec<String>,
    pub binhost_mirrors: Vec<String>,
}

#[derive(Debug)]
pub struct BinPkg {
    pub cpv: String,
    pub slot: String,
    pub repo: String,
    pub path: String,
}

#[derive(Debug)]
pub struct BinPkgInfo {
    pub cpv: String,
    pub slot: String,
    pub repo: String,
    pub path: String,
    pub tar_size: usize,
    pub metadata: HashMap<String, String>,
}

impl BinTree {
    pub fn new(root: &str) -> Self {
        BinTree {
            root: root.to_string(),
            pkgdir: format!("{}/usr/portage/packages", root),
            binhost: vec![],
            binhost_mirrors: vec![],
        }
    }

    pub fn with_binhost(root: &str, binhost: Vec<String>, binhost_mirrors: Vec<String>) -> Self {
        BinTree {
            root: root.to_string(),
            pkgdir: format!("{}/usr/portage/packages", root),
            binhost,
            binhost_mirrors,
        }
    }

    pub async fn get_all_binpkgs(&self) -> Result<Vec<String>, InvalidData> {
        let path = Path::new(&self.pkgdir);
        if !path.exists() {
            return Ok(vec![]);
        }
        let mut cpvs = vec![];
        let mut entries = fs::read_dir(path).await.map_err(|e| InvalidData::new(&format!("Failed to read pkgdir: {}", e), None))?;
        while let Some(entry) = entries.next_entry().await.map_err(|e| InvalidData::new(&format!("Failed to read entry: {}", e), None))? {
            let path = entry.path();
            let metadata = fs::metadata(&path).await.map_err(|e| InvalidData::new(&format!("Failed to read metadata: {}", e), None))?;
            if metadata.is_file() && path.extension().and_then(|e| e.to_str()) == Some("tbz2") {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    // Remove .tbz2 extension to get cpv
                    if let Some(cpv) = name.strip_suffix(".tbz2") {
                        cpvs.push(cpv.to_string());
                    }
                }
            }
        }
        Ok(cpvs)
    }

    pub async fn get_binpkg_info(&self, cpv: &str) -> Result<Option<BinPkg>, InvalidData> {
        match self.parse_tbz2(cpv).await? {
            Some(info) => Ok(Some(BinPkg {
                cpv: info.cpv,
                slot: info.slot,
                repo: info.repo,
                path: info.path,
            })),
            None => Ok(None),
        }
    }

    pub fn is_available(&self, cpv: &str) -> bool {
        Path::new(&self.pkgdir).join(format!("{}.tbz2", cpv)).exists()
    }

    /// Check if binary package is available from binhost
    pub async fn is_available_from_binhost(&self, cpv: &str) -> bool {
        if self.binhost.is_empty() {
            return false;
        }

        // Try each binhost URL
        for base_url in &self.binhost {
            let url = format!("{}/{}.tbz2", base_url.trim_end_matches('/'), cpv);
            if self.check_binhost_url(&url).await {
                return true;
            }
        }

        // Try mirrors
        for base_url in &self.binhost_mirrors {
            let url = format!("{}/{}.tbz2", base_url.trim_end_matches('/'), cpv);
            if self.check_binhost_url(&url).await {
                return true;
            }
        }

        false
    }

    /// Check if a binhost URL exists (HEAD request)
    async fn check_binhost_url(&self, url: &str) -> bool {
        // For now, we'll use curl as a simple check
        // In a real implementation, you'd use an HTTP client
        match tokio::process::Command::new("curl")
            .args(&["--head", "--silent", "--fail", url])
            .output()
            .await {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    /// Fetch binary package from binhost
    pub async fn fetch_from_binhost(&self, cpv: &str) -> Result<(), InvalidData> {
        if self.binhost.is_empty() {
            return Err(InvalidData::new("No binhost configured", None));
        }

        // Ensure pkgdir exists
        fs::create_dir_all(&self.pkgdir)
            .await
            .map_err(|e| InvalidData::new(&format!("Failed to create pkgdir: {}", e), None))?;

        let local_path = Path::new(&self.pkgdir).join(format!("{}.tbz2", cpv));

        // Try each binhost URL
        for base_url in &self.binhost {
            let url = format!("{}/{}.tbz2", base_url.trim_end_matches('/'), cpv);
            if self.download_binhost_package(&url, &local_path).await? {
                return Ok(());
            }
        }

        // Try mirrors
        for base_url in &self.binhost_mirrors {
            let url = format!("{}/{}.tbz2", base_url.trim_end_matches('/'), cpv);
            if self.download_binhost_package(&url, &local_path).await? {
                return Ok(());
            }
        }

        Err(InvalidData::new(&format!("Binary package {} not found on any binhost", cpv), None))
    }

    /// Download binary package from URL
    async fn download_binhost_package(&self, url: &str, local_path: &Path) -> Result<bool, InvalidData> {
        println!("Fetching {} from {}", local_path.file_name().unwrap().to_string_lossy(), url);

        match tokio::process::Command::new("curl")
            .args(&["--silent", "--fail", "-o"])
            .arg(local_path)
            .arg(url)
            .output()
            .await {
            Ok(output) if output.status.success() => {
                println!("Successfully downloaded {}", local_path.display());
                Ok(true)
            },
            _ => Ok(false), // Try next URL
        }
    }

    /// Parse a .tbz2 binary package and extract metadata
    pub async fn parse_tbz2(&self, cpv: &str) -> Result<Option<BinPkgInfo>, InvalidData> {
        let pkg_path = Path::new(&self.pkgdir).join(format!("{}.tbz2", cpv));
        if !pkg_path.exists() {
            return Ok(None);
        }

        let mut file = fs::File::open(&pkg_path)
            .await
            .map_err(|e| InvalidData::new(&format!("Failed to open {}: {}", pkg_path.display(), e), None))?;

        // Read the entire file
        let mut data = Vec::new();
        file.read_to_end(&mut data)
            .await
            .map_err(|e| InvalidData::new(&format!("Failed to read {}: {}", pkg_path.display(), e), None))?;

        // Find XPAK data at the end
        let xpak_start = data.windows(8).rposition(|window| window == b"XPAKPACK");
        let xpak_stop = data.windows(8).rposition(|window| window == b"XPAKSTOP");

        if xpak_start.is_none() || xpak_stop.is_none() {
            return Err(InvalidData::new("Invalid .tbz2 format: missing XPAK data", None));
        }

        let xpak_start = xpak_start.unwrap();
        let xpak_stop = xpak_stop.unwrap();

        if xpak_stop <= xpak_start {
            return Err(InvalidData::new("Invalid .tbz2 format: XPAKSTOP before XPAKPACK", None));
        }

        // Extract XPAK data
        let xpak_data = &data[xpak_start..=xpak_stop + 7];

        // Parse XPAK
        let (index, xpak_data_part) = match xpak::xsplit_mem(xpak_data) {
            Some(result) => result,
            None => return Err(InvalidData::new("Failed to parse XPAK data", None)),
        };

        // Extract metadata from XPAK
        let mut metadata = HashMap::new();
        let keys = xpak::getindex_mem(&index);

        for key in keys {
            if let Some(value_bytes) = xpak::getitem((&index, &xpak_data_part), &key) {
                if let Ok(value_str) = String::from_utf8(value_bytes) {
                    metadata.insert(key, value_str);
                }
            }
        }

        // Extract basic info
        let slot = metadata.get("SLOT").unwrap_or(&"0".to_string()).clone();
        let repo = metadata.get("repository").unwrap_or(&"gentoo".to_string()).clone();

        // The tar.bz2 part is everything before XPAK
        let tar_size = xpak_start;

        Ok(Some(BinPkgInfo {
            cpv: cpv.to_string(),
            slot,
            repo,
            path: pkg_path.to_string_lossy().to_string(),
            tar_size,
            metadata,
        }))
    }
}