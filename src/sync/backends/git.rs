use crate::sync::{SyncBackend, SyncError, SyncResult};
use tokio::process::Command;
use std::path::Path;

pub struct GitSync;

impl GitSync {
    pub fn new() -> Self {
        GitSync
    }
}

#[async_trait::async_trait]
impl SyncBackend for GitSync {
    fn name(&self) -> &'static str {
        "GitSync"
    }

    fn short_desc(&self) -> &'static str {
        "Perform sync operations on git based repositories"
    }

    async fn exists(&self, repo_path: &Path) -> bool {
        repo_path.join(".git").exists()
    }

    async fn new_repo(&self, repo: &crate::porttree::Repository) -> Result<SyncResult, SyncError> {
        let repo_path = Path::new(&repo.location);
        
        if !repo_path.exists() {
            tokio::fs::create_dir_all(repo_path).await?;
        }

        let sync_uri = repo.sync_uri.as_deref().ok_or_else(|| {
            SyncError::Repository("No sync URI configured for git repository".to_string())
        })?;

        let mut clone_cmd = Command::new("git");
        clone_cmd.arg("clone");
        
        if let Some(depth) = repo.sync_depth {
            if depth > 0 {
                clone_cmd.arg("--depth").arg(depth.to_string());
            }
        } else {
            clone_cmd.arg("--depth").arg("1");
        }

        clone_cmd.arg("--quiet")
            .arg(sync_uri)
            .arg(".")
            .current_dir(repo_path);

        let output = clone_cmd.output().await?;
        
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SyncError::Command(format!("git clone failed: {}", stderr)));
        }

        Ok(SyncResult {
            success: true,
            message: format!("Successfully cloned {}", repo.name),
            changes: true,
        })
    }

    async fn sync(&self, repo: &crate::porttree::Repository) -> Result<SyncResult, SyncError> {
        let repo_path = Path::new(&repo.location);

        if !self.exists(repo_path).await {
            return self.new_repo(repo).await;
        }

        let mut fetch_cmd = Command::new("git");
        fetch_cmd.arg("fetch")
            .arg("--quiet")
            .current_dir(repo_path);

        let output = fetch_cmd.output().await?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(SyncError::Command(format!("git fetch failed: {}", stderr)));
        }

        let mut merge_cmd = Command::new("git");
        merge_cmd.arg("merge")
            .arg("--ff-only")
            .arg("--quiet")
            .arg("@{u}")
            .current_dir(repo_path);

        let merge_output = merge_cmd.output().await?;
        
        let changes = !merge_output.stdout.is_empty() || !merge_output.stderr.is_empty();
        
        if !merge_output.status.success() {
            let stderr = String::from_utf8_lossy(&merge_output.stderr);
            if stderr.contains("diverged") {
                return Err(SyncError::Repository(
                    format!("Repository has diverged from upstream: {}", repo.name)
                ));
            }
            return Err(SyncError::Command(format!("git merge failed: {}", stderr)));
        }

        Ok(SyncResult {
            success: true,
            message: format!("Successfully synced {} via git", repo.name),
            changes,
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
    fn test_git_sync_creation() {
        let sync = GitSync::new();
        assert_eq!(sync.name(), "GitSync");
        assert_eq!(sync.short_desc(), "Perform sync operations on git based repositories");
    }

    #[tokio::test]
    async fn test_git_exists_no_repo() {
        let temp_dir = TempDir::new().unwrap();
        let sync = GitSync::new();
        
        assert!(!sync.exists(temp_dir.path()).await);
    }

    #[tokio::test]
    async fn test_git_exists_with_git_dir() {
        let temp_dir = TempDir::new().unwrap();
        let git_dir = temp_dir.path().join(".git");
        tokio::fs::create_dir(&git_dir).await.unwrap();
        
        let sync = GitSync::new();
        assert!(sync.exists(temp_dir.path()).await);
    }

    #[tokio::test]
    async fn test_new_repo_no_uri() {
        let temp_dir = TempDir::new().unwrap();
        let sync = GitSync::new();
        
        let repo = Repository {
            name: "test".to_string(),
            location: temp_dir.path().to_str().unwrap().to_string(),
            sync_type: Some("git".to_string()),
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
