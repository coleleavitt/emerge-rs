use crate::sync::{SyncBackend, SyncError, SyncResult};
use tokio::process::Command;
use std::path::Path;

pub struct CvsSync;

impl CvsSync {
    pub fn new() -> Self {
        CvsSync
    }
}

#[async_trait::async_trait]
impl SyncBackend for CvsSync {
    fn name(&self) -> &'static str {
        "CvsSync"
    }

    fn short_desc(&self) -> &'static str {
        "Perform sync operations on cvs based repositories"
    }

    async fn exists(&self, repo_path: &Path) -> bool {
        repo_path.join("CVS").exists()
    }

    async fn new_repo(&self, repo: &crate::porttree::Repository) -> Result<SyncResult, SyncError> {
        let repo_path = Path::new(&repo.location);
        
        if !repo_path.exists() {
            tokio::fs::create_dir_all(repo_path).await?;
        }

        let sync_uri = repo.sync_uri.as_deref().ok_or_else(|| {
            SyncError::Repository("No sync URI configured for cvs repository".to_string())
        })?;

        let mut checkout_cmd = Command::new("cvs");
        checkout_cmd.arg("-d")
            .arg(sync_uri)
            .arg("checkout")
            .arg("-P")
            .arg(".")
            .current_dir(repo_path);

        let result = checkout_cmd.output().await?;
        
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(SyncError::Command(format!("cvs checkout failed: {}", stderr)));
        }

        Ok(SyncResult {
            success: true,
            message: format!("Successfully created {} via cvs", repo.name),
            changes: true,
        })
    }

    async fn sync(&self, repo: &crate::porttree::Repository) -> Result<SyncResult, SyncError> {
        let repo_path = Path::new(&repo.location);
        
        if !self.exists(repo_path).await {
            return self.new_repo(repo).await;
        }

        let mut update_cmd = Command::new("cvs");
        update_cmd.arg("update")
            .arg("-d")
            .arg("-P")
            .current_dir(repo_path);

        let result = update_cmd.output().await?;
        
        let changes = !result.stdout.is_empty() || !result.stderr.is_empty();
        
        if !result.status.success() {
            let stderr = String::from_utf8_lossy(&result.stderr);
            return Err(SyncError::Command(format!("cvs update failed: {}", stderr)));
        }

        Ok(SyncResult {
            success: true,
            message: format!("Successfully synced {} via cvs", repo.name),
            changes,
        })
    }
}
