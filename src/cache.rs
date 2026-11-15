pub mod local_novel;
#[cfg(feature = "network")]
pub mod network_novel;
pub use local_novel::LocalNovelCache;
#[cfg(feature = "network")]
pub use network_novel::NetworkNovelCache;
#[cfg(feature = "network")]
pub mod book_source;
#[cfg(feature = "network")]
pub mod source_state;
#[cfg(feature = "network")]
pub use source_state::{load_source_state, save_source_state, source_state_path};
pub mod history;
pub use history::*;
pub mod setting;
pub use setting::*;
