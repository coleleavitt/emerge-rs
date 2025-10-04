use crate::sync::{SyncBackend, SyncError, SyncResult};
use tokio::process::Command;
use std::path::Path;

pub struct SvnSync;

impl SvnSync {
    pub fn new() -> Self {
        SvnSync
    }
}

#[async_trait::async_trait]
impl SyncBackend for SvnSync {
    fn name(&self) -> &'static str {
        "SvnSync"
    }

    fn short_desc(&self) -> &'static str {
        "Perform sync operations on svn based repositories"
    }

    async fn exists(&self, repo_path: &Path) -> bool {
        repo_path.join(".svn").exists()
    }

    async fn new_repo(&self, repo: &crate::porttree::Repository) -> Result<SyncResult, SyncError> {
        let repo_path = Path::new(&repo.location);
        
        if !repo_path.exists() {
            tokio::fs::create_dir_all(repo_path).await?;
        }

        let sync_uri = repo.sync_uri.as_deref().ok_or_else(|| {
            SyncError::Repository("No sync URI configured for svn repository".to_string())
        })?;

        let mut checkout_cmd = Command::new("svn");
        checkout_cmd.arg("checkout")
            .arg("--quiet")
            .arg(sync_uri)
            .arg(".")
            .current_dir(repo_path);

        let result = checkout_cmd.output().await?;
        
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(SyncError::Command(format!("svn checkout failed: {}", stderr)));
        }

        Ok(SyncResult {
            success: true,
            message: format!("Successfully created {} via svn", repo.name),
            changes: true,
        })
    }

    async fn sync(&self, repo: &crate::porttree::Repository) -> Result<SyncResult, SyncError> {
        let repo_path = Path::new(&repo.location);
        
        if !self.exists(repo_path).await {
            return self.new_repo(repo).await;
        }

        let mut update_cmd = Command::new("svn");
        update_cmd.arg("update")
            .arg("--quiet")
            .current_dir(repo_path);

        let result = update_cmd.output().await?;
        
        let changes = !result.stdout.is_empty() || !result.stderr.is_empty();
        
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(SyncError::Command(format!("svn update failed: {}", stderr)));
        }

        Ok(SyncResult {
            success: true,
            message: format!("Successfully synced {} via svn", repo.name),
            changes,
        })
    }
}
