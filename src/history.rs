use anyhow::Result;
use chrono::{DateTime, Local};
use serde::{Deserialize, Serialize};
use std::{fs::File, path::PathBuf};

use crate::{
    novel::{TxtNovel, TxtNovelCache},
    utils::novel_catch_dir,
};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct History {
    pub histories: Vec<(PathBuf, HistoryItem)>,
}

impl History {
    const MAX_LEN: usize = 100;
    pub fn get_catch_file_path() -> Result<PathBuf> {
        Ok(PathBuf::new().join(novel_catch_dir()?).join("history.json"))
    }

    pub fn load() -> Result<Self> {
        match File::open(Self::get_catch_file_path()?) {
            Ok(file) => Ok(serde_json::from_reader(file)?),
            Err(_) => Ok(Self { histories: vec![] }),
        }
    }

    pub fn save(&self) -> Result<()> {
        let file = File::create(Self::get_catch_file_path()?)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    pub fn add(&mut self, path: PathBuf, history_item: HistoryItem) {
        match self.histories.iter().position(|item| item.0 == path) {
            Some(index) => {
                self.histories.remove(index);
                self.histories.insert(0, (path, history_item));
            }
            None => {
                self.histories.insert(0, (path, history_item));
                if self.histories.len() > Self::MAX_LEN {
                    self.histories.pop();
                }
            }
        }
    }

    pub fn remove(&mut self, path: PathBuf) {
        if let Some(index) = self.histories.iter().position(|item| item.0 == path) {
            self.histories.remove(index);
        }
    }

    pub fn remove_index(&mut self, index: usize) {
        self.histories.remove(index);
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct HistoryItem {
    pub current_chapter: String,
    pub last_read_at: DateTime<Local>,
    pub percent: f64,
    pub file_name: String,
}

impl From<TxtNovelCache> for HistoryItem {
    fn from(value: TxtNovelCache) -> Self {
        let (current_chapter, percent) = if value.chapter_offset.is_empty() {
            ("".to_string(), value.line_percent * 100.0)
        } else {
            (
                value.chapter_offset[value.current_chapter].0.clone(),
                ((value.current_chapter as f64 / value.chapter_offset.len() as f64) * 100.0)
                    .round(),
            )
        };

        Self {
            current_chapter,
            last_read_at: Local::now(),
            percent,
            file_name: value
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
        }
    }
}

impl From<&TxtNovel> for HistoryItem {
    fn from(value: &TxtNovel) -> Self {
        let (current_chapter, percent) = if value.chapter_offset.is_empty() {
            ("".to_string(), value.line_percent * 100.0)
        } else {
            (
                value.chapter_offset[value.current_chapter].0.clone(),
                ((value.current_chapter as f64 / value.chapter_offset.len() as f64) * 100.0)
                    .round(),
            )
        };

        Self {
            current_chapter,
            last_read_at: Local::now(),
            percent,
            file_name: value
                .path
                .file_name()
                .unwrap()
                .to_string_lossy()
                .to_string(),
        }
    }
}

impl Drop for History {
    fn drop(&mut self) {
        self.save().expect("历史记录保存失败");
    }
}
