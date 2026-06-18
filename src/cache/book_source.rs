use std::{
    fs::File,
    ops::{Deref, DerefMut},
    path::PathBuf,
};

use crate::{Result, utils::novel_catch_dir};
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
            Ok(file) => {
                // 缓存结构为 {"book_sources": [<BookSource>...]}。不能直接反序列化:裸 serde
                // 会跳过命名 fontMap 的展开(把 `"fontMap":"名"` 字符串引用当 BTreeMap 解析而
                // 失败,报 "untagged enum Rule" 不匹配),导致外部生成/手改的 v2 缓存加载失败。
                // 改走 BookSource::from_value_many(与 from_path / trn import 一致),逐源展开后
                // 再反序列化,保证有效 v2 源能 round-trip 进缓存。
                let value: serde_json::Value = serde_json::from_reader(file)?;
                let sources_value = match value {
                    serde_json::Value::Object(mut map) => map
                        .remove("book_sources")
                        .unwrap_or_else(|| serde_json::Value::Array(vec![])),
                    // 容错:允许缓存文件本身就是一个书源数组。
                    other => other,
                };
                let book_sources = BookSource::from_value_many(sources_value)
                    .map_err(parse_book_source::BookSourceError::from)?;
                Ok(BookSourceCache { book_sources })
            }
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
        self.book_sources
            .iter()
            .position(|item| item.url == book_source_url && item.name == book_source_name)
    }

    pub fn add_book_source(&mut self, book_source: BookSource) {
        if let Some(index) = self.find_book_source_index(&book_source.url, &book_source.name) {
            self.book_sources.remove(index);
        }

        self.book_sources.push(book_source);
    }

    pub fn find_book_source(
        &self,
        book_source_url: &str,
        book_source_name: &str,
    ) -> Option<&BookSource> {
        self.book_sources
            .iter()
            .find(|bs| bs.url == book_source_url && bs.name == book_source_name)
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
