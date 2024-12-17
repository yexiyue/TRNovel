pub mod local_history_item;
pub use local_history_item::LocalHistoryItem;
pub mod network_history_item;
pub use network_history_item::NetworkHistoryItem;
use serde::{Deserialize, Serialize};

use crate::cache::{LocalNovelCache, NetworkNovelCache};

/// 历史记录，分网络和本地，只用于在UI中展示数据
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum HistoryItem {
    Local(LocalHistoryItem),
    Network(NetworkHistoryItem),
}

impl From<LocalNovelCache> for HistoryItem {
    fn from(item: LocalNovelCache) -> Self {
        HistoryItem::Local(item.into())
    }
}

impl From<NetworkNovelCache> for HistoryItem {
    fn from(item: NetworkNovelCache) -> Self {
        HistoryItem::Network(item.into())
    }
}
