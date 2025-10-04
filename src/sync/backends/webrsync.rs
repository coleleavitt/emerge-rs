use crate::sync::{SyncBackend, SyncError, SyncResult};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tokio::fs;

pub struct WebRsyncSync;

impl WebRsyncSync {
    pub fn new() -> Self {
        WebRsyncSync
    }

    async fn download_snapshot(uri: &str, dest: &Path) -> Result<PathBuf, SyncError> {
        let snapshot_url = format!("{}/portage-latest.tar.xz", uri.trim_end_matches('/'));
        let snapshot_file = dest.join("portage-latest.tar.xz");

        let output = Command::new("wget")
            .arg("--quiet")
            .arg("--timeout=180")
            .arg("--tries=3")
            .arg("-O")
            .arg(&snapshot_file)
            .arg(&snapshot_url)
            .output()
            .await
            .map_err(|e| SyncError::Command(format!("Failed to execute wget: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SyncError::Network(format!("Failed to download snapshot: {}", stderr)));
        }

        Ok(snapshot_file)
    }

    async fn extract_snapshot(snapshot: &Path, dest: &Path) -> Result<(), SyncError> {
        let output = Command::new("tar")
            .arg("-xJf")
            .arg(snapshot)
            .arg("-C")
            .arg(dest)
            .arg("--strip-components=1")
            .output()
            .await
            .map_err(|e| SyncError::Command(format!("Failed to execute tar: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SyncError::Command(format!("Failed to extract snapshot: {}", stderr)));
        }

        Ok(())
    }

    async fn verify_snapshot(snapshot: &Path, signature_uri: &str) -> Result<(), SyncError> {
        let sig_file = format!("{}.gpgsig", snapshot.display());
        let sig_url = format!("{}.gpgsig", signature_uri);

        let output = Command::new("wget")
            .arg("--quiet")
            .arg("--timeout=60")
            .arg("-O")
            .arg(&sig_file)
            .arg(&sig_url)
            .output()
            .await
            .map_err(|e| SyncError::Command(format!("Failed to download signature: {}", e)))?;

        if !output.status.success() {
            return Err(SyncError::Validation("Signature file not available".to_string()));
        }

        let verify_output = Command::new("gpg")
            .arg("--verify")
            .arg(&sig_file)
            .arg(snapshot)
            .output()
            .await
            .map_err(|e| SyncError::Command(format!("Failed to verify signature: {}", e)))?;

        if !verify_output.status.success() {
            let stderr = String::from_utf8_lossy(&verify_output.stderr);
            return Err(SyncError::Validation(format!("Signature verification failed: {}", stderr)));
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl SyncBackend for WebRsyncSync {
    fn name(&self) -> &'static str {
        "WebRsyncSync"
    }

    fn short_desc(&self) -> &'static str {
        "Perform sync operations on webrsync based repositories"
    }

    async fn exists(&self, repo_path: &Path) -> bool {
        repo_path.exists()
    }

    async fn new_repo(&self, repo: &crate::porttree::Repository) -> Result<SyncResult, SyncError> {
        let uri = repo.sync_uri.as_ref()
            .ok_or_else(|| SyncError::Repository("No sync-uri specified".to_string()))?;

        let repo_path = Path::new(&repo.location);
        fs::create_dir_all(repo_path)
            .await
            .map_err(|e| SyncError::IO(e))?;

        let temp_dir = repo_path.parent()
            .ok_or_else(|| SyncError::Repository("Invalid repository path".to_string()))?
            .join(".webrsync-temp");

        fs::create_dir_all(&temp_dir)
            .await
            .map_err(|e| SyncError::IO(e))?;

        let snapshot = Self::download_snapshot(uri, &temp_dir).await?;
        
        Self::extract_snapshot(&snapshot, repo_path).await?;

        fs::remove_file(&snapshot)
            .await
            .map_err(|e| SyncError::IO(e))?;

        Ok(SyncResult {
            success: true,
            message: format!("Successfully created repository from webrsync snapshot"),
            changes: true,
        })
    }

    async fn sync(&self, repo: &crate::porttree::Repository) -> Result<SyncResult, SyncError> {
        let uri = repo.sync_uri.as_ref()
            .ok_or_else(|| SyncError::Repository("No sync-uri specified".to_string()))?;

        let repo_path = Path::new(&repo.location);
        
        if !repo_path.exists() {
            return self.new_repo(repo).await;
        }

        let temp_dir = repo_path.parent()
            .ok_or_else(|| SyncError::Repository("Invalid repository path".to_string()))?
            .join(".webrsync-temp");

        fs::create_dir_all(&temp_dir)
            .await
            .map_err(|e| SyncError::IO(e))?;

        let snapshot = Self::download_snapshot(uri, &temp_dir).await?;

        let backup_dir = repo_path.parent()
            .ok_or_else(|| SyncError::Repository("Invalid repository path".to_string()))?
            .join(format!(".{}-backup", repo.name));

        if backup_dir.exists() {
            fs::remove_dir_all(&backup_dir)
                .await
                .map_err(|e| SyncError::IO(e))?;
        }

        fs::rename(repo_path, &backup_dir)
            .await
            .map_err(|e| SyncError::IO(e))?;

        fs::create_dir_all(repo_path)
            .await
            .map_err(|e| SyncError::IO(e))?;

        match Self::extract_snapshot(&snapshot, repo_path).await {
            Ok(_) => {
                fs::remove_dir_all(&backup_dir)
                    .await
                    .map_err(|e| SyncError::IO(e))?;

                fs::remove_file(&snapshot)
                    .await
                    .map_err(|e| SyncError::IO(e))?;

                Ok(SyncResult {
                    success: true,
                    message: format!("Successfully synced repository from webrsync snapshot"),
                    changes: true,
                })
            }
            Err(e) => {
                fs::remove_dir_all(repo_path)
                    .await
                    .ok();
                
                fs::rename(&backup_dir, repo_path)
                    .await
                    .map_err(|e| SyncError::IO(e))?;

                Err(e)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porttree::Repository;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn test_webrsync_creation() {
        let sync = WebRsyncSync::new();
        assert_eq!(sync.name(), "WebRsyncSync");
        assert_eq!(sync.short_desc(), "Perform sync operations on webrsync based repositories");
    }

    #[tokio::test]
    async fn test_webrsync_exists_no_repo() {
        let temp_dir = TempDir::new().unwrap();
        let non_existent = temp_dir.path().join("nonexistent");
        let sync = WebRsyncSync::new();
        
        assert!(!sync.exists(&non_existent).await);
    }

    #[tokio::test]
    async fn test_webrsync_exists_with_dir() {
        let temp_dir = TempDir::new().unwrap();
        let sync = WebRsyncSync::new();
        
        assert!(sync.exists(temp_dir.path()).await);
    }

    #[tokio::test]
    async fn test_new_repo_no_uri() {
        let temp_dir = TempDir::new().unwrap();
        let sync = WebRsyncSync::new();
        
        let repo = Repository {
            name: "test".to_string(),
            location: temp_dir.path().to_str().unwrap().to_string(),
            sync_type: Some("webrsync".to_string()),
            sync_uri: None,
            auto_sync: true,
            sync_depth: None,
            sync_hooks_only_on_change: false,
            sync_metadata: crate::porttree::SyncMetadata {
                last_sync: None,
                last_attempt: None,
                success: false,
                error_message: None,
            },
            eclass_cache: HashMap::new(),
            metadata_cache: HashMap::new(),
        };

        let result = sync.new_repo(&repo).await;
        assert!(result.is_err());
        match result {
            Err(SyncError::Repository(msg)) => {
                assert!(msg.contains("No sync-uri"));
            }
            _ => panic!("Expected Repository error"),
        }
    }
}
