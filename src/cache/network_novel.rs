use crate::{
    errors::Errors,
    novel::{network_novel::NetworkNovel, Novel},
    utils::{get_md5_string, novel_catch_dir},
    Result,
};
use parse_book_source::BookListItem;
use serde::{Deserialize, Serialize};
use std::{fmt::Display, fs::File, path::PathBuf};

/// 历史记录（一个用于展示，一个用于缓存，方便下次快速访问）
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct NetworkNovelCache {
    #[serde(flatten)]
    pub book_list_item: BookListItem,
    pub book_source_url: String,
    pub book_source_name: String,
    pub current_chapter: usize,
    pub current_chapter_name: String,
    pub line_percent: f64,
    pub chapter_percent: f64,
}

impl NetworkNovelCache {
    pub fn save(&self) -> Result<()> {
        let cache_path = Self::cache_path(&self.book_list_item.book_url)?;
        let file = File::create(cache_path)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    pub fn cache_path<T: Display>(url: T) -> Result<PathBuf> {
        let novel_catch_dir = PathBuf::new().join(novel_catch_dir()?).join("network");

        if !novel_catch_dir.exists() {
            std::fs::create_dir_all(&novel_catch_dir)?;
        }

        Ok(novel_catch_dir
            .join(get_md5_string(url))
            .with_extension("json"))
    }
}

// 从本地小说创建缓存
impl TryFrom<&NetworkNovel> for NetworkNovelCache {
    type Error = Errors;
    fn try_from(value: &NetworkNovel) -> Result<Self> {
        let novel_chapters = value.novel_chapters.clone();
        let parser = value.book_source.try_lock().unwrap();

        Ok(Self {
            current_chapter: novel_chapters.current_chapter,
            current_chapter_name: value.get_current_chapter_name()?,
            line_percent: novel_chapters.line_percent,
            book_list_item: value.book_list_item.clone(),
            book_source_url: parser.book_source.book_source_url.clone(),
            book_source_name: parser.book_source.book_source_name.clone(),
            chapter_percent: (value.current_chapter as f64
                / value.get_chapters_result()?.len() as f64)
                * 100.0,
        })
    }
}

// 从路径加载缓存
impl TryFrom<&str> for NetworkNovelCache {
    type Error = Errors;
    fn try_from(value: &str) -> Result<Self> {
        let cache_path = Self::cache_path(value)?;
        let file = File::open(cache_path)?;
        Ok(serde_json::from_reader(file)?)
    }
}
