use crate::{
    utils::{get_md5_string, novel_catch_dir},
    Result,
};
use chrono::{DateTime, Local};
use parse_book_source::BookListItem;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, fs::File, path::PathBuf};

/// 历史记录（一个用于展示，一个用于缓存，方便下次快速访问）
#[derive(Debug, Serialize, Deserialize)]
pub struct NetworkNovelCache {
    pub book_info: BookListItem,
    pub last_read_at: DateTime<Local>,
    pub current_chapter: usize,
    pub line_percent: f64,
}

impl NetworkNovelCache {
    pub fn from<T: Display>(value: T) -> Result<Self> {
        let cache_path = Self::path_to_catch(value)?;
        let file = File::open(cache_path)?;
        Ok(serde_json::from_reader(file)?)
    }

    pub fn save(&self) -> Result<()> {
        let cache_path = Self::path_to_catch(&self.book_info.book_url)?;
        let file = File::create(cache_path)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    pub fn path_to_catch<T: Display>(path: T) -> Result<PathBuf> {
        Ok(PathBuf::new()
            .join(novel_catch_dir()?)
            .join(get_md5_string(path))
            .with_extension("json"))
    }
}
