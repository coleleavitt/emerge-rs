use crate::sync::{SyncBackend, SyncError, SyncResult};
use tokio::process::Command;
use std::path::Path;

pub struct RsyncSync;

impl RsyncSync {
    pub fn new() -> Self {
        RsyncSync
    }
}

#[async_trait::async_trait]
impl SyncBackend for RsyncSync {
    fn name(&self) -> &'static str {
        "RsyncSync"
    }

    fn short_desc(&self) -> &'static str {
        "Perform sync operations on rsync based repositories"
    }

    async fn exists(&self, repo_path: &Path) -> bool {
        repo_path.exists()
    }

    async fn new_repo(&self, repo: &crate::porttree::Repository) -> Result<SyncResult, SyncError> {
        self.sync(repo).await
    }

    async fn sync(&self, repo: &crate::porttree::Repository) -> Result<SyncResult, SyncError> {
        let repo_path = Path::new(&repo.location);
        
        tokio::fs::create_dir_all(repo_path).await?;

        let sync_uri = repo.sync_uri.as_deref().ok_or_else(|| {
            SyncError::Repository("No sync URI configured for rsync repository".to_string())
        })?;

        let mut rsync_cmd = Command::new("rsync");
        rsync_cmd
            .arg("--recursive")
            .arg("--links")
            .arg("--safe-links")
            .arg("--perms")
            .arg("--times")
            .arg("--compress")
            .arg("--force")
            .arg("--whole-file")
            .arg("--delete")
            .arg("--stats")
            .arg("--human-readable")
            .arg("--timeout=180")
            .arg("--exclude=/.git")
            .arg("--quiet")
            .arg(sync_uri)
            .arg(&repo.location);

        let output = rsync_cmd.output().await?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SyncError::Command(format!("rsync failed: {}", stderr)));
        }

        let changes = !output.stdout.is_empty();

        Ok(SyncResult {
            success: true,
            message: format!("Successfully synced {} via rsync", repo.name),
            changes: true,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::porttree::Repository;
    use std::collections::HashMap;
    use tempfile::TempDir;

    #[test]
    fn test_rsync_sync_creation() {
        let sync = RsyncSync::new();
        assert_eq!(sync.name(), "RsyncSync");
        assert_eq!(sync.short_desc(), "Perform sync operations on rsync based repositories");
    }

    #[tokio::test]
    async fn test_rsync_exists_no_repo() {
        let temp_dir = TempDir::new().unwrap();
        let non_existent = temp_dir.path().join("nonexistent");
        let sync = RsyncSync::new();
        
        assert!(!sync.exists(&non_existent).await);
    }

    #[tokio::test]
    async fn test_rsync_exists_with_dir() {
        let temp_dir = TempDir::new().unwrap();
        let sync = RsyncSync::new();
        
        assert!(sync.exists(temp_dir.path()).await);
    }

    #[tokio::test]
    async fn test_new_repo_no_uri() {
        let temp_dir = TempDir::new().unwrap();
        let sync = RsyncSync::new();
        
        let repo = Repository {
            name: "test".to_string(),
            location: temp_dir.path().to_str().unwrap().to_string(),
            sync_type: Some("rsync".to_string()),
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
                assert!(msg.contains("No sync URI"));
            }
            _ => panic!("Expected Repository error"),
        }
    }
}
