//! 操作规则:搜索 / 浏览 / 书详情 / 目录 / 正文 的字段抽取与分页配置,以及黄金样例。

use super::http::{PreStep, Request};
use super::rule::Rule;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// 一本书的字段抽取规则(均可省略)。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BookRules {
    /// 列表项:指向书详情页的链接(搜索/浏览结果用;bookInfo 阶段忽略)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book_url: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intro: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_chapter: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toc_url: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word_count: Option<Rule>,
}

/// 书详情操作:详情字段抽取(同 [`BookRules`])+ 可选前置请求链(见 design D7-bis)。
/// 字段与 `BookRules` 同名同序,故现有 `bookInfo:{...}` JSON 逐字节解析等价;引擎经
/// [`BookInfoOp::as_book_rules`] 复用既有 `eval_book_info`(不用 `flatten` 以保 `deny_unknown_fields`)。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BookInfoOp {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub book_url: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intro: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_chapter: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub toc_url: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub word_count: Option<Rule>,
    /// 详情主请求前的前置请求链;空 = 现状(直接 fetch book_url)。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prelude: Vec<PreStep>,
}

impl BookInfoOp {
    /// 取详情字段抽取规则视图(供引擎复用 `eval_book_info(&BookRules)`,不暴露 `prelude`)。
    pub fn as_book_rules(&self) -> BookRules {
        BookRules {
            book_url: self.book_url.clone(),
            name: self.name.clone(),
            author: self.author.clone(),
            cover: self.cover.clone(),
            intro: self.intro.clone(),
            kind: self.kind.clone(),
            last_chapter: self.last_chapter.clone(),
            toc_url: self.toc_url.clone(),
            word_count: self.word_count.clone(),
        }
    }
}

/// 共享列表页规格:search 与 explore 取一页书共用。承载前置链、请求(render / interceptApi /
/// readyFor / totalPages / hasMore / pageBy / vars 均在 [`Request`] 上)、列表选择与每项字段抽取。
/// 引擎以 `run_list_page(spec, vars, page, page_size)` 统一执行。
///
/// 这正是历史 `SearchOp` 的形状——前序 change 已把全部取页旋钮收敛到 [`Request`],故 search/explore
/// 共用同一规格,无需各维护一套列表求值分支。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ListPageSpec {
    /// 主请求之前按序执行的前置请求链(见 design D7-bis);空 = 单发(现状)。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prelude: Vec<PreStep>,
    pub request: Request,
    pub list: Rule,
    pub item: BookRules,
}

/// 搜索操作 == 共享列表页规格(裸用,不包 `page` 层;search JSON 形状不变)。
pub type SearchOp = ListPageSpec;

/// 一个静态入口:固定标题 + 取页变量(字面量)。供「全部·最热」这类固定入口声明。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct StaticEntry {
    pub title: String,
    /// 取页变量:与 base/page/pageSize 合并后运行 `explore.page`。值为字面量。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, String>,
}

/// fetch 入口项的生成规则:标题 + 变量。两者对「当前数据项(ctx)+ 外层 forEach 变量(vars)」求值,
/// 故 `title`/`vars` 规则既能 `via:json` 读数据项字段,也能 `{{name}}` 引用循环变量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FetchEntryItem {
    pub title: Rule,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, Rule>,
}

/// 远端抓取入口源:请求远端分类数据 → `list` 抽数组 → 每项经 `item` 生成入口。
/// 可选 `forEach` 按多组变量重复请求并合并入口(空 = 执行一次)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct FetchEntrySource {
    /// 循环变量集:对每组变量各请求一次、合并结果;变量在请求模板与 `item` 规则中可见。空 = 一次。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub for_each: Vec<BTreeMap<String, String>>,
    /// 入口数据请求(复用 [`Request`]:支持 render / interceptApi / charset / headers / 模板)。
    pub request: Request,
    /// 从响应抽取数据项数组(每项作为 `item` 规则的上下文)。
    pub list: Rule,
    pub item: FetchEntryItem,
}

/// 入口源:静态固定入口 或 远端抓取入口。`explore.entries` 是其数组,按声明顺序合并
/// (无独立 chain 类型——「按序合并」即数组遍历)。与 [`Rule`] 一致用 untagged + 唯一键判别
/// (`static` / `fetch`)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum EntrySource {
    /// 静态入口列表。
    Static {
        #[serde(rename = "static")]
        static_entries: Vec<StaticEntry>,
    },
    /// 远端抓取动态入口(`Box`:`FetchEntrySource` 含完整 `Request`,避免枚举变体尺寸悬殊;
    /// `Box<T>` 对 serde/schemars 透明,JSON 与 schema 形状不变)。
    Fetch { fetch: Box<FetchEntrySource> },
}

/// 浏览操作:两阶段——`entries` 生成可选择的入口,`page` 用选中入口的变量取书。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExploreOp {
    /// 入口源数组(按声明顺序合并;static + fetch 组合)。
    pub entries: Vec<EntrySource>,
    /// 共享列表页规格:用选中入口变量 + page/pageSize 取书。
    pub page: ListPageSpec,
}

fn default_max_pages() -> u32 {
    100
}

/// 目录规则(章节 + 分卷 + 可选分页)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct TocRules {
    /// 目录主请求之前按序执行的前置请求链(见 design D7-bis);空 = 现状。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prelude: Vec<PreStep>,
    pub list: Rule,
    pub name: Rule,
    pub url: Rule,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_volume: Option<Rule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page: Option<Rule>,
    #[serde(default = "default_max_pages")]
    pub max_pages: u32,
}

/// 正文规则(可选分页)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ContentRules {
    /// 正文主请求之前按序执行的前置请求链(见 design D7-bis);空 = 现状。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prelude: Vec<PreStep>,
    pub value: Rule,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub next_page: Option<Rule>,
    #[serde(default = "default_max_pages")]
    pub max_pages: u32,
}

/// 样例期望不变量。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Expect {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_chapters: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volumes: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_content_chars: Option<usize>,
}

/// 黄金样例。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Sample {
    pub book_url: String,
    #[serde(default)]
    pub expect: Expect,
}
