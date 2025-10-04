use crate::sync::{SyncBackend, SyncError, SyncResult};
use tokio::process::Command;
use std::path::Path;

pub struct MercurialSync;

impl MercurialSync {
    pub fn new() -> Self {
        MercurialSync
    }
}

#[async_trait::async_trait]
impl SyncBackend for MercurialSync {
    fn name(&self) -> &'static str {
        "MercurialSync"
    }

    fn short_desc(&self) -> &'static str {
        "Perform sync operations on mercurial based repositories"
    }

    async fn exists(&self, repo_path: &Path) -> bool {
        repo_path.join(".hg").exists()
    }

    async fn new_repo(&self, repo: &crate::porttree::Repository) -> Result<SyncResult, SyncError> {
        let repo_path = Path::new(&repo.location);
        
        if !repo_path.exists() {
            tokio::fs::create_dir_all(repo_path).await?;
        }

        let sync_uri = repo.sync_uri.as_deref().ok_or_else(|| {
            SyncError::Repository("No sync URI configured for mercurial repository".to_string())
        })?;

        let mut clone_cmd = Command::new("hg");
        clone_cmd.arg("clone")
            .arg("--quiet")
            .arg(sync_uri)
            .arg(".")
            .current_dir(repo_path);

        let result = clone_cmd.output().await?;
        
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(SyncError::Command(format!("hg clone failed: {}", stderr)));
        }

        Ok(SyncResult {
            success: true,
            message: format!("Successfully created {} via mercurial", repo.name),
            changes: true,
        })
    }

    async fn sync(&self, repo: &crate::porttree::Repository) -> Result<SyncResult, SyncError> {
        let repo_path = Path::new(&repo.location);
        
        if !self.exists(repo_path).await {
            return self.new_repo(repo).await;
        }

        let mut pull_cmd = Command::new("hg");
        pull_cmd.arg("pull")
            .arg("--quiet")
            .arg("--update")
            .current_dir(repo_path);

        let result = pull_cmd.output().await?;
        
        let changes = !result.stdout.is_empty() || !result.stderr.is_empty();
        
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(SyncError::Command(format!("hg pull failed: {}", stderr)));
        }

        Ok(SyncResult {
            success: true,
            message: format!("Successfully synced {} via mercurial", repo.name),
            changes,
        })
    }
}
