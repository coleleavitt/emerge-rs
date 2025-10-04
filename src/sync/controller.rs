use crate::sync::{SyncError, SyncResult};
use crate::sync::backends::Backend;
use crate::porttree::Repository;

pub async fn sync_repository(repo: &Repository) -> Result<SyncResult, SyncError> {
    let sync_type = repo.sync_type.as_deref().unwrap_or("rsync");
    
    let backend = Backend::new(sync_type)
        .ok_or_else(|| SyncError::Repository(format!("Unsupported sync type: {}", sync_type)))?;
    
    backend.sync(repo).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use crate::porttree::SyncMetadata;

    #[tokio::test]
    async fn test_sync_repository_unsupported_type() {
        let repo = Repository {
            name: "test".to_string(),
            location: "/tmp/test".to_string(),
            sync_type: Some("unknown".to_string()),
            sync_uri: None,
            auto_sync: true,
            sync_depth: None,
            sync_hooks_only_on_change: false,
            sync_metadata: SyncMetadata {
                last_sync: None,
                last_attempt: None,
                success: false,
                error_message: None,
            },
            eclass_cache: HashMap::new(),
            metadata_cache: HashMap::new(),
        };

        let result = sync_repository(&repo).await;
        assert!(result.is_err());
        match result {
            Err(SyncError::Repository(msg)) => {
                assert!(msg.contains("Unsupported sync type"));
            }
            _ => panic!("Expected Repository error"),
        }
    }

    #[tokio::test]
    async fn test_sync_repository_defaults_to_rsync() {
        let repo = Repository {
            name: "test".to_string(),
            location: "/tmp/test".to_string(),
            sync_type: None,
            sync_uri: None,
            auto_sync: true,
            sync_depth: None,
            sync_hooks_only_on_change: false,
            sync_metadata: SyncMetadata {
                last_sync: None,
                last_attempt: None,
                success: false,
                error_message: None,
            },
            eclass_cache: HashMap::new(),
            metadata_cache: HashMap::new(),
        };

        let result = sync_repository(&repo).await;
        assert!(result.is_err());
        match result {
            Err(SyncError::Repository(msg)) => {
                assert!(msg.contains("No sync URI"));
            }
            _ => {}
        }
    }
}
