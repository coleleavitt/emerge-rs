// ebuild module - Rust implementation of Portage ebuild helpers
//
// This module replaces the bash-based ebuild.sh with a Rust implementation

pub mod helpers;
pub mod phases;
pub mod environment;
pub mod eclass;
pub mod portage_helpers;
pub mod install_helpers;
pub mod use_helpers;
pub mod build_helpers;
pub mod version;
pub mod build_system;
pub mod bash_parser;
pub mod src_uri;
pub mod archive;
pub mod native_phases;
pub mod eapi;
pub mod download;

// Re-export only non-conflicting items
pub use environment::*;
pub use phases::*;
pub use version::*;
pub use build_system::*;
pub use src_uri::*;
pub use archive::*;
pub use native_phases::*;
pub use eapi::*;

// For helpers, we need to be selective to avoid conflicts
pub use helpers::{einfo, ewarn, ebegin, default_src_prepare, default_src_unpack};
pub use portage_helpers::{eerror, eend};
pub use install_helpers::{dobin, doins};
pub use use_helpers::*;
pub use build_helpers::*;
