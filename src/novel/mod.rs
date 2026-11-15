pub mod local_novel;
#[cfg(feature = "network")]
pub mod network_novel;
pub mod novel_core;
pub mod toc_rule;
pub use novel_core::*;
