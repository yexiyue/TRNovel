use std::{
    fs::File,
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use crate::{utils::novel_catch_dir, Result};
use parse_book_source::BookSource;
use serde::{Deserialize, Serialize};

/// 书源支持
/// 本地文件导入
/// 网络链接导入，考虑网络是每次都走请求还是走缓存
/// 可以是数组也可以是一个对象
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BookSourceCache {
    pub book_sources: Vec<BookSource>,
}

impl BookSourceCache {
    pub fn get_cache_file_path() -> Result<PathBuf> {
        Ok(PathBuf::new()
            .join(novel_catch_dir()?)
            .join("book_sources.json"))
    }

    pub fn load() -> Result<Self> {
        match File::open(Self::get_cache_file_path()?) {
            Ok(file) => Ok(serde_json::from_reader(file)?),
            Err(_) => Ok(Default::default()),
        }
    }

    pub fn save(&self) -> Result<()> {
        let file = File::create(Self::get_cache_file_path()?)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    pub fn find_book_source_index(
        &self,
        book_source_url: &str,
        book_source_name: &str,
    ) -> Option<usize> {
        self.book_sources.iter().position(|item| {
            item.book_source_url == book_source_url && item.book_source_name == book_source_name
        })
    }

    pub fn add_book_source(&mut self, book_source: BookSource) {
        if let Some(index) =
            self.find_book_source_index(&book_source.book_source_url, &book_source.book_source_name)
        {
            self.book_sources.remove(index);
        }

        self.book_sources.push(book_source);
    }

    pub fn find_book_source(
        &self,
        book_source_url: &str,
        book_source_name: &str,
    ) -> Option<&BookSource> {
        self.book_sources.iter().find(|bs| {
            bs.book_source_url == book_source_url && bs.book_source_name == book_source_name
        })
    }
}

impl Deref for BookSourceCache {
    type Target = Vec<BookSource>;

    fn deref(&self) -> &Self::Target {
        &self.book_sources
    }
}

impl DerefMut for BookSourceCache {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.book_sources
    }
}

impl Drop for BookSourceCache {
    fn drop(&mut self) {
        self.save().expect("save book source cache failed");
    }
}
