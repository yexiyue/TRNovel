//! 操作规则:搜索 / 浏览 / 书详情 / 目录 / 正文 的字段抽取与分页配置,以及黄金样例。

use super::http::{PreStep, Request};
use super::rule::{Rule, UrlOrRule};
use serde::{Deserialize, Serialize};

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

/// 搜索操作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SearchOp {
    /// 主请求之前按序执行的前置请求链(见 design D7-bis);空 = 单发(现状)。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prelude: Vec<PreStep>,
    pub request: Request,
    pub list: Rule,
    pub item: BookRules,
}

/// 浏览分类。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Category {
    pub title: String,
    pub url: UrlOrRule,
}

/// 浏览操作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExploreOp {
    /// 分类请求之前按序执行的前置请求链(见 design D7-bis);空 = 现状。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prelude: Vec<PreStep>,
    pub categories: Vec<Category>,
    pub list: Rule,
    pub item: BookRules,
    /// 渲染取页(对齐 search 的 `Request.render`;explore 各分类共享)。为真则用受控浏览器
    /// 渲染分类 URL、跑站点 JS,SPA 浏览/分类列表才取得到数据(否则 reqwest 直取,现状)。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub render: bool,
    /// 渲染就绪 CSS 选择器。无 `intercept_api` 时取渲染后 DOM(方式 A);与 `intercept_api`
    /// 共存时(`render-dual-source`)作 DOM 就绪闸,供 `via:css` 的 `total_pages` 对 DOM 求值。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_for: Option<String>,
    /// CDP 拦截目标响应 URL 子串(方式 B:拦签名 API 响应体,交 `via:"json"` 规则)。可与 `ready_for` 共存。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intercept_api: Option<String>,
    /// 精确总页数规则(`render-dual-source`):对取页结果求值得「共 M 页」(翻页进度)。
    /// `via:json` 对 API body 求值;`via:css`/`xpath` 对渲染 DOM 求值(需 `intercept_api` +
    /// `ready_for` 共存)。空 = 不返回总数。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_pages: Option<Rule>,
    /// 翻页边界规则(`list-has-more`):对取页结果求值「是否还有下一页」(非空且非 `false`/`0` → 有)。
    /// 源路由同 `total_pages`(按 `via`)。空 = 不提供边界(UI 不限制)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<Rule>,
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
