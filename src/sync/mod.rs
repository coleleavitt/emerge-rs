pub mod backends;
pub mod controller;

use crate::exception::InvalidData;
use std::fmt;

#[derive(Debug)]
pub enum SyncError {
    Network(String),
    Repository(String),
    Command(String),
    Validation(String),
    Timeout(String),
    IO(std::io::Error),
}

impl fmt::Display for SyncError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            SyncError::Network(msg) => write!(f, "Network error: {}", msg),
            SyncError::Repository(msg) => write!(f, "Repository error: {}", msg),
            SyncError::Command(msg) => write!(f, "Command error: {}", msg),
            SyncError::Validation(msg) => write!(f, "Validation error: {}", msg),
            SyncError::Timeout(msg) => write!(f, "Timeout error: {}", msg),
            SyncError::IO(err) => write!(f, "IO error: {}", err),
        }
    }
}

impl std::error::Error for SyncError {}

impl From<std::io::Error> for SyncError {
    fn from(err: std::io::Error) -> Self {
        SyncError::IO(err)
    }
}

pub struct SyncResult {
    pub success: bool,
    pub message: String,
    pub changes: bool,
}

#[async_trait::async_trait]
pub trait SyncBackend {
    fn name(&self) -> &'static str;
    fn short_desc(&self) -> &'static str;
    
    async fn exists(&self, repo_path: &std::path::Path) -> bool;
    async fn sync(&self, repo: &crate::porttree::Repository) -> Result<SyncResult, SyncError>;
    async fn new_repo(&self, repo: &crate::porttree::Repository) -> Result<SyncResult, SyncError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sync_error_display() {
        let network_err = SyncError::Network("Connection failed".to_string());
        assert!(network_err.to_string().contains("Network error"));
        assert!(network_err.to_string().contains("Connection failed"));

        let repo_err = SyncError::Repository("Invalid repo".to_string());
        assert!(repo_err.to_string().contains("Repository error"));

        let cmd_err = SyncError::Command("Command failed".to_string());
        assert!(cmd_err.to_string().contains("Command error"));

        let val_err = SyncError::Validation("Validation failed".to_string());
        assert!(val_err.to_string().contains("Validation error"));

        let timeout_err = SyncError::Timeout("Timed out".to_string());
        assert!(timeout_err.to_string().contains("Timeout error"));
    }

    #[test]
    fn test_sync_result() {
        let result = SyncResult {
            success: true,
            message: "Test message".to_string(),
            changes: true,
        };

        assert!(result.success);
        assert_eq!(result.message, "Test message");
        assert!(result.changes);
    }

    #[test]
    fn test_sync_error_from_io() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let sync_err = SyncError::from(io_err);
        
        match sync_err {
            SyncError::IO(_) => {},
            _ => panic!("Expected IO error"),
        }
    }
}
