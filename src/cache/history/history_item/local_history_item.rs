use crate::cache::LocalNovelCache;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalHistoryItem {
    pub current_chapter: String,
    pub last_read_at: DateTime<Local>,
    // 小说阅读进度
    pub percent: f64,
    // 小说标题
    pub title: String,
}

impl From<LocalNovelCache> for LocalHistoryItem {
    fn from(value: LocalNovelCache) -> Self {
        let (current_chapter, percent) = if value.chapters.is_empty() {
            ("".to_string(), value.line_percent * 100.0)
        } else {
            (
                value.chapters[value.current_chapter].0.clone(),
                (value.current_chapter as f64 / value.chapters.len() as f64) * 100.0,
            )
        };

        Self {
            current_chapter,
            last_read_at: Local::now(),
            percent,
            title: value
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
        }
    }
}
