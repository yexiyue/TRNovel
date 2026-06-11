//! 领域类型(纯数据,无 IO、无规则逻辑)。

use serde::{Deserialize, Serialize};

/// 目录条目(章节;`is_volume` 为 true 时表示卷标题,不是可阅读章节)。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Chapter {
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub is_volume: bool,
}

/// 卷标记(分卷元数据):卷标题 + 其首章在扁平章节列表中的索引。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct Volume {
    pub title: String,
    pub first_chapter_index: usize,
}

/// 解析后的目录:扁平章节列表 + 平行卷元数据(卷不进入章节序列)。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Toc {
    pub chapters: Vec<Chapter>,
    pub volumes: Vec<Volume>,
}

/// 书籍详情。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookInfo {
    pub name: String,
    pub author: String,
    pub cover: String,
    pub intro: String,
    pub kind: String,
    pub last_chapter: String,
    pub toc_url: String,
    pub word_count: String,
}

/// 搜索/浏览结果中的一本书(书籍详情 + 入口 URL)。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BookListItem {
    #[serde(flatten)]
    pub info: BookInfo,
    pub book_url: String,
}

/// 搜索/浏览**一页**的结果:书列表 + 可选精确总页数(`render-dual-source`,翻页进度「第 N / M 页」;
/// 书源未配 `totalPages` 或取不到则为 `None`)。`explore`/`search` 的返回类型。
///
/// (后续 `list-has-more` 在此 additive 加 `has_more: Option<bool>` 翻页边界。)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BookList {
    pub items: Vec<BookListItem>,
    pub total_pages: Option<u32>,
}

impl BookList {
    /// 仅书列表、无总页数(非 render / 未配 `totalPages` 的现状路径)。
    pub fn new(items: Vec<BookListItem>) -> Self {
        Self {
            items,
            total_pages: None,
        }
    }
}
