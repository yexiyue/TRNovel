use std::path::Path;

use crate::Result;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
pub mod rule;
pub use rule::*;
pub mod json_source;
pub use json_source::*;
use serde_json::Value;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BookSource {
    pub book_source_group: String,
    pub book_source_name: String,
    pub book_source_url: String,
    pub enabled_explore: bool,
    pub explore_url: Option<String>,
    pub header: String,
    pub last_update_time: u64,
    pub respond_time: u64,
    pub rule_book_info: RuleBookInfo,
    pub rule_content: RuleContent,
    pub rule_explore: Option<RuleExplore>,
    pub rule_search: RuleSearch,
    pub rule_toc: RuleToc,
    pub search_url: String,
}

impl BookSource {
    pub async fn from_url(url: &str) -> Result<Vec<Self>> {
        let res: Value = reqwest::get(url).await?.json().await?;
        Self::from_json(res)
    }

    pub fn from_path<T: AsRef<Path>>(path: T) -> Result<Vec<Self>> {
        let file = std::fs::File::open(path)?;
        let res: Value = serde_json::from_reader(file)?;
        Self::from_json(res)
    }

    pub fn from_json(value: Value) -> Result<Vec<Self>> {
        if value.is_object() {
            Ok(vec![serde_json::from_value(value)?])
        } else if value.is_array() {
            Ok(serde_json::from_value(value)?)
        } else {
            Err(anyhow!("value is not object or array").into())
        }
    }
}
