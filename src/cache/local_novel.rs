use crate::errors::Result;
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
    pub fn from<T: AsRef<Path>>(value: T) -> Result<Self> {
        let cache_path = Self::path_to_catch(value)?;
        let file = File::open(cache_path)?;
        Ok(serde_json::from_reader(file)?)
    }

    pub fn save(&self) -> Result<()> {
        let cache_path = Self::path_to_catch(&self.path)?;
        let file = File::create(cache_path)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    pub fn path_to_catch<T: AsRef<Path>>(path: T) -> Result<PathBuf> {
        Ok(PathBuf::new()
            .join(novel_catch_dir()?)
            .join(get_path_md5(path)?)
            .with_extension("json"))
    }
}
