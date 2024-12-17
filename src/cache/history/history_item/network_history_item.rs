use crate::cache::NetworkNovelCache;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetworkHistoryItem {
    pub current_chapter: String,
    pub last_read_at: DateTime<Local>,
    // 小说阅读进度
    pub percent: f64,
    // 小说标题
    pub title: String,
    pub book_source: String,
}

impl From<NetworkNovelCache> for NetworkHistoryItem {
    fn from(value: NetworkNovelCache) -> Self {
        Self {
            current_chapter: value.current_chapter_name,
            last_read_at: Local::now(),
            percent: value.chapter_percent,
            title: value.book_list_item.name,
            book_source: value.book_source_name,
        }
    }
}
