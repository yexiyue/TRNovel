use crate::errors::{Errors, Result};
use crate::novel::Novel;
use crate::novel::local_novel::LocalNovel;
use crate::utils::{get_path_md5, novel_catch_dir};

use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LocalNovelCache {
    pub chapters: Vec<(String, usize)>,
    pub encoding: &'static encoding_rs::Encoding,
    pub current_chapter: usize,
    pub line_percent: f64,
    pub path: PathBuf,
}

impl LocalNovelCache {
    pub fn save(&self) -> Result<()> {
        let cache_path = Self::cache_path(&self.path)?;
        let file = File::create(cache_path)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    pub fn cache_path<T: AsRef<Path>>(path: T) -> Result<PathBuf> {
        let novel_catch_dir = PathBuf::new().join(novel_catch_dir()?).join("local");

        if !novel_catch_dir.exists() {
            std::fs::create_dir_all(&novel_catch_dir)?;
        }

        Ok(novel_catch_dir
            .join(get_path_md5(path)?)
            .with_extension("json"))
    }
}

// 从本地小说创建缓存
impl TryFrom<&LocalNovel> for LocalNovelCache {
    type Error = Errors;
    fn try_from(value: &LocalNovel) -> Result<Self> {
        let novel_chapters = value.novel_chapters.clone();
        Ok(Self {
            chapters: value.get_chapters_result()?.to_vec(),
            encoding: value.encoding,
            current_chapter: novel_chapters.current_chapter,
            path: value.path.clone(),
            line_percent: novel_chapters.line_percent,
        })
    }
}

// 从路径加载缓存
impl TryFrom<&Path> for LocalNovelCache {
    type Error = Errors;
    fn try_from(value: &Path) -> Result<Self> {
        let cache_path = Self::cache_path(value)?;
        let file = File::open(cache_path)?;
        Ok(serde_json::from_reader(file)?)
    }
}
