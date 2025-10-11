use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HttpConfig {
    pub timeout: Option<u64>,
    pub header: Option<HashMap<String, String>>,
    /// 请求速率限制（令牌桶算法）
    pub rate_limit: Option<RateLimit>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct RateLimit {
    /// 每秒钟最大请求次数
    pub max_count: u64,
    /// 每隔多少秒补充一次令牌
    pub fill_duration: f64,
}
