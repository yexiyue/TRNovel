use chrono::{DateTime, Local};
use parse_book_source::{BookInfo, BookListItem, Chapter, ChapterList};
use serde::{Deserialize, Serialize};

/// 历史记录（一个用于展示，一个用于缓存，方便下次快速访问）
#[derive(Debug, Serialize, Deserialize)]
pub struct BookHistory {
    // 通过book_url来区分
    pub book_info: BookListItem,
    pub current_chapter: Chapter,
    pub last_read_at: DateTime<Local>,
    // 阅读进度()
    pub percent: f64,
}
