use crate::Result;
use anyhow::anyhow;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

pub mod http_config;
pub use http_config::*;
pub mod rule;
pub use rule::*;

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct BookSource {
    pub book_source_group: String,
    pub book_source_name: String,
    pub book_source_url: String,
    pub last_update_time: u64,

    pub search_url: String,
    pub explore_url: Option<String>,
    pub rule_explore_item: Option<RuleExploreItem>,

    pub header: Option<String>,
    pub respond_time: Option<u64>,

    #[serde(default)]
    pub http_config: HttpConfig,

    // 解析规则
    pub rule_book_info: RuleBookInfo,
    pub rule_content: RuleContent,
    pub rule_explore: Option<RuleSearch>,
    pub rule_search: RuleSearch,
    pub rule_toc: RuleToc,
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
            Ok(value
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|item| serde_json::from_value(item.clone()).ok())
                .collect())
        } else {
            Err(anyhow!("value is not object or array").into())
        }
    }
}
