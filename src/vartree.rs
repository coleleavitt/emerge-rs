// vartree.rs -- Installed package database (/var/db/pkg)

use std::collections::HashMap;
use tokio::fs;
use std::path::Path;
use crate::exception::InvalidData;

#[derive(Debug)]
pub struct VarTree {
    pub root: String,
    pub dbpath: String,
}

#[derive(Debug)]
pub struct VarPkg {
    pub cpv: String,
    pub slot: String,
    pub contents: Vec<String>, // file paths
    pub environment: HashMap<String, String>,
}

impl VarTree {
    pub fn new(root: &str) -> Self {
        VarTree {
            root: root.to_string(),
            dbpath: format!("{}/var/db/pkg", root),
        }
    }

    pub async fn get_all_installed(&self) -> Result<Vec<String>, InvalidData> {
        let path = Path::new(&self.dbpath);
        if !path.exists() {
            return Ok(vec![]);
        }
        let mut cpvs = vec![];

        // Read category directories
        let mut category_entries = fs::read_dir(path).await.map_err(|e| InvalidData::new(&format!("Failed to read db: {}", e), None))?;
        while let Some(category_entry) = category_entries.next_entry().await.map_err(|e| InvalidData::new(&format!("Failed to read category entry: {}", e), None))? {
            let category_path = category_entry.path();
            let metadata = fs::metadata(&category_path).await.map_err(|e| InvalidData::new(&format!("Failed to read metadata: {}", e), None))?;
            if metadata.is_dir() {
                // Read package-version directories within each category
                let mut pkg_entries = fs::read_dir(&category_path).await.map_err(|e| InvalidData::new(&format!("Failed to read category {}: {}", category_path.display(), e), None))?;
                while let Some(pkg_entry) = pkg_entries.next_entry().await.map_err(|e| InvalidData::new(&format!("Failed to read package entry: {}", e), None))? {
                    let pkg_path = pkg_entry.path();
                    let pkg_metadata = fs::metadata(&pkg_path).await.map_err(|e| InvalidData::new(&format!("Failed to read pkg metadata: {}", e), None))?;
                    if pkg_metadata.is_dir() {
                        if let Some(name) = pkg_path.file_name().and_then(|n| n.to_str()) {
                            // name is like "package-version", we need to prepend "category-"
                            if let Some(category_name) = category_path.file_name().and_then(|n| n.to_str()) {
                                let cpv = format!("{}-{}", category_name, name);
                                cpvs.push(cpv);
                            }
                        }
                    }
                }
            }
        }
        Ok(cpvs)
    }

    pub async fn get_pkg_info(&self, cpv: &str) -> Result<Option<VarPkg>, InvalidData> {
        let pkg_path = Path::new(&self.dbpath).join(cpv);
        if !pkg_path.exists() {
            return Ok(None);
        }

        let contents_path = pkg_path.join("CONTENTS");
        let contents = if contents_path.exists() {
            fs::read_to_string(&contents_path)
                .await
                .map_err(|e| InvalidData::new(&format!("Failed to read CONTENTS: {}", e), None))?
                .lines()
                .map(|l| l.to_string())
                .collect()
        } else {
            vec![]
        };

        let environment_path = pkg_path.join("environment.bz2");
        let environment = if environment_path.exists() {
            // Placeholder: decompress and parse
            HashMap::new()
        } else {
            HashMap::new()
        };

        let slot = fs::read_to_string(pkg_path.join("SLOT"))
            .await
            .unwrap_or_else(|_| "0".to_string())
            .trim()
            .to_string();

        Ok(Some(VarPkg {
            cpv: cpv.to_string(),
            slot,
            contents,
            environment,
        }))
    }

    pub fn is_installed(&self, cpv: &str) -> bool {
        Path::new(&self.dbpath).join(cpv).exists()
    }
}