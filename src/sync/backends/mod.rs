pub mod cvs;
pub mod git;
pub mod mercurial;
pub mod rsync;
pub mod svn;
pub mod webrsync;

use crate::sync::{SyncBackend, SyncResult, SyncError};
use crate::porttree::Repository;
use std::path::Path;

pub enum Backend {
    Cvs(cvs::CvsSync),
    Git(git::GitSync),
    Mercurial(mercurial::MercurialSync),
    Rsync(rsync::RsyncSync),
    Svn(svn::SvnSync),
    WebRsync(webrsync::WebRsyncSync),
}

impl Backend {
    pub fn new(sync_type: &str) -> Option<Self> {
        match sync_type {
            "cvs" => Some(Backend::Cvs(cvs::CvsSync::new())),
            "git" => Some(Backend::Git(git::GitSync::new())),
            "mercurial" | "hg" => Some(Backend::Mercurial(mercurial::MercurialSync::new())),
            "rsync" => Some(Backend::Rsync(rsync::RsyncSync::new())),
            "svn" => Some(Backend::Svn(svn::SvnSync::new())),
            "webrsync" => Some(Backend::WebRsync(webrsync::WebRsyncSync::new())),
            _ => None,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Backend::Cvs(b) => b.name(),
            Backend::Git(b) => b.name(),
            Backend::Mercurial(b) => b.name(),
            Backend::Rsync(b) => b.name(),
            Backend::Svn(b) => b.name(),
            Backend::WebRsync(b) => b.name(),
        }
    }

    pub fn short_desc(&self) -> &'static str {
        match self {
            Backend::Cvs(b) => b.short_desc(),
            Backend::Git(b) => b.short_desc(),
            Backend::Mercurial(b) => b.short_desc(),
            Backend::Rsync(b) => b.short_desc(),
            Backend::Svn(b) => b.short_desc(),
            Backend::WebRsync(b) => b.short_desc(),
        }
    }

    pub async fn exists(&self, repo_path: &Path) -> bool {
        match self {
            Backend::Cvs(b) => b.exists(repo_path).await,
            Backend::Git(b) => b.exists(repo_path).await,
            Backend::Mercurial(b) => b.exists(repo_path).await,
            Backend::Rsync(b) => b.exists(repo_path).await,
            Backend::Svn(b) => b.exists(repo_path).await,
            Backend::WebRsync(b) => b.exists(repo_path).await,
        }
    }

    pub async fn sync(&self, repo: &Repository) -> Result<SyncResult, SyncError> {
        match self {
            Backend::Cvs(b) => b.sync(repo).await,
            Backend::Git(b) => b.sync(repo).await,
            Backend::Mercurial(b) => b.sync(repo).await,
            Backend::Rsync(b) => b.sync(repo).await,
            Backend::Svn(b) => b.sync(repo).await,
            Backend::WebRsync(b) => b.sync(repo).await,
        }
    }

    pub async fn new_repo(&self, repo: &Repository) -> Result<SyncResult, SyncError> {
        match self {
            Backend::Cvs(b) => b.new_repo(repo).await,
            Backend::Git(b) => b.new_repo(repo).await,
            Backend::Mercurial(b) => b.new_repo(repo).await,
            Backend::Rsync(b) => b.new_repo(repo).await,
            Backend::Svn(b) => b.new_repo(repo).await,
            Backend::WebRsync(b) => b.new_repo(repo).await,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_backend_creation_git() {
        let backend = Backend::new("git");
        assert!(backend.is_some());
        if let Some(Backend::Git(_)) = backend {
        } else {
            panic!("Expected Git backend");
        }
    }

    #[test]
    fn test_backend_creation_rsync() {
        let backend = Backend::new("rsync");
        assert!(backend.is_some());
        if let Some(Backend::Rsync(_)) = backend {
        } else {
            panic!("Expected Rsync backend");
        }
    }

    #[test]
    fn test_backend_creation_webrsync() {
        let backend = Backend::new("webrsync");
        assert!(backend.is_some());
        if let Some(Backend::WebRsync(_)) = backend {
        } else {
            panic!("Expected WebRsync backend");
        }
    }

    #[test]
    fn test_backend_creation_unknown() {
        let backend = Backend::new("unknown");
        assert!(backend.is_none());
    }

    #[test]
    fn test_backend_name() {
        let git = Backend::new("git").unwrap();
        assert_eq!(git.name(), "GitSync");

        let rsync = Backend::new("rsync").unwrap();
        assert_eq!(rsync.name(), "RsyncSync");

        let webrsync = Backend::new("webrsync").unwrap();
        assert_eq!(webrsync.name(), "WebRsyncSync");
    }

    #[test]
    fn test_backend_short_desc() {
        let git = Backend::new("git").unwrap();
        assert!(git.short_desc().contains("git"));

        let rsync = Backend::new("rsync").unwrap();
        assert!(rsync.short_desc().contains("rsync"));

        let webrsync = Backend::new("webrsync").unwrap();
        assert!(webrsync.short_desc().contains("webrsync"));
    }
}
