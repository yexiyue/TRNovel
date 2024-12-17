use parse_book_source::BookSource;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BookSourceCache {
    #[serde(flatten)]
    pub book_sources: Vec<BookSource>,
}
