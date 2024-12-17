pub mod local_novel;
pub mod network_novel;
pub use local_novel::LocalNovelCache;
pub use network_novel::NetworkNovelCache;
pub mod book_source;
pub mod history;
pub use history::*;
