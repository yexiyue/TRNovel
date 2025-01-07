use crate::utils::novel_catch_dir;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::{fs::File, path::PathBuf};
pub mod history_item;
pub use history_item::*;

/// 历史记录
/// Vec<(ID, 历史记录)>
/// ID:
/// - 本地为小说路径
/// - 网络为book链接
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct History {
    pub local_path: Option<PathBuf>,
    pub histories: Vec<(String, HistoryItem)>,
}

impl History {
    const MAX_LEN: usize = 100;
    pub fn get_cache_file_path() -> Result<PathBuf> {
        Ok(PathBuf::new().join(novel_catch_dir()?).join("history.json"))
    }

    pub fn load() -> Result<Self> {
        match File::open(Self::get_cache_file_path()?) {
            Ok(file) => Ok(serde_json::from_reader(file)?),
            Err(_) => Ok(Self {
                histories: vec![],
                local_path: None,
            }),
        }
    }

    pub fn save(&self) -> Result<()> {
        let file = File::create(Self::get_cache_file_path()?)?;
        serde_json::to_writer_pretty(file, self)?;
        Ok(())
    }

    pub fn add(&mut self, path: &str, history_item: HistoryItem) {
        match self.histories.iter().position(|item| item.0 == path) {
            Some(index) => {
                self.histories.remove(index);
                self.histories.insert(0, (path.into(), history_item));
            }
            None => {
                self.histories.insert(0, (path.into(), history_item));
                if self.histories.len() > Self::MAX_LEN {
                    self.histories.pop();
                }
            }
        }
    }

    pub fn remove(&mut self, path: &str) {
        if let Some(index) = self.histories.iter().position(|item| item.0 == path) {
            self.histories.remove(index);
        }
    }

    pub fn remove_index(&mut self, index: usize) {
        self.histories.remove(index);
    }
}
