//! HTTP 配置与请求类型:字符集/重试/限速/取页模式、登录表单、请求与多步编排(前置链 + 命名捕获)。

use super::rule::{Rule, UrlOrRule};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// 字符集。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Charset {
    #[default]
    Auto,
    Utf8,
    Gbk,
    Gb18030,
    Big5,
}

/// 重试策略。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Retry {
    #[serde(default)]
    pub max: u32,
    #[serde(default)]
    pub backoff_ms: u64,
}

/// 速率限制。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RateLimit {
    pub max_count: u64,
    pub per_ms: u64,
}

/// 取页模式:是否动用浏览器解反爬挑战。
/// 真正是否开浏览器还需 app/用户级授权(两级取交集,见 OpenSpec change `browser-fetcher` D12)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum FetchMode {
    /// 默认:平时 reqwest,撞挑战才升级浏览器。
    #[default]
    Auto,
    /// 永不开浏览器,撞挑战即降级。
    Reqwest,
    /// 整站强制走浏览器(首请求即被挑战 / 整页 JS 渲染)。
    Browser,
}

/// HTTP 配置块。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Http {
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// 静态 cookie;也是运行时注入 clearance cookie 的落点。
    #[serde(default)]
    pub cookies: HashMap<String, String>,
    /// 先 GET 这些页以预热会话 cookie。
    #[serde(default)]
    pub warmup: Vec<String>,
    #[serde(default)]
    pub charset: Charset,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<Retry>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit: Option<RateLimit>,
    /// 取页模式(auto|reqwest|browser);默认 auto。
    #[serde(default)]
    pub fetcher: FetchMode,
}

/// 声明式登录表单项类型。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum RowUiType {
    /// 单行文本。
    #[default]
    Text,
    /// 密码(TUI 掩码显示)。
    Password,
    /// 下拉选择(配合 `options`)。
    Select,
    /// 布尔开关。
    Toggle,
}

/// 声明式登录表单的一行(TUI 渲染对应控件,收集值加密存为 loginInfo)。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct RowUi {
    /// 字段名(也是 loginInfo 中的 key)。
    pub name: String,
    #[serde(rename = "type", default)]
    pub ui_type: RowUiType,
    /// `select` 类型的候选项。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

/// HTTP 方法。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Method {
    #[default]
    Get,
    Post,
}

/// 多步编排:捕获变量的作用域(三级级联 章节→书籍→书源,见 design D7-bis)。
/// 默认 `Chapter`——最短寿命、零持久、零跨书外溢;`get` 时按 章节→书籍→书源 取第一个非空。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum VarScope {
    /// 仅本次 op 调用(search/explore/bookInfo/toc/content)内存活,调用结束消亡(默认)。
    /// 一次性 csrf/sign/cursor 用它。
    #[default]
    Chapter,
    /// 随 per-book 快照持久化(由 app 注入·导出);如详情/列表 token 复用到 toc/content。
    /// 注意:**search/explore 的 `prelude` 阶段尚无 per-book 载体**(用户选书后才建 per-book 引擎),
    /// 在那里用 `book` 会写进会被丢弃的实例 → 静默无效;search 阶段的 token 请用 `source`/`chapter`。
    Book,
    /// 书源级长存(引擎内跨 op 共享);如全站 API host/版本/全站 csrf。
    /// 跨会话持久仅在 app 接线落盘时保证(默认构建为进程内)。
    Source,
}

/// 一条结构化命名捕获:对**所属请求的响应**用 `value` 规则求一个字符串,
/// 写入 `scope` 指定的作用域层,后续步骤/抽取规则以 `{{name}}` 引用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Capture {
    /// 变量名(后续以 `{{name}}` 引用,走现有模板插值)。
    pub name: String,
    /// 对响应求值的规则(复用 `Rule` AST,产物是字符串)。
    pub value: Rule,
    /// 写入哪一层作用域;默认 `chapter`(本次调用临时)。
    #[serde(default)]
    pub scope: VarScope,
}

/// 前置请求链中的一步:一个请求 + 其响应上的有序命名捕获(见 design D7-bis)。
/// 本步 url/headers/body 可引用更早步骤捕获的 `{{name}}`。显式列字段(**不**用 `#[serde(flatten)]`
/// 内嵌 [`Request`]:`Rule` 为 untagged 兜底,flatten 会令 `deny_unknown_fields` 校验失效)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct PreStep {
    pub url: UrlOrRule,
    #[serde(default)]
    pub method: Method,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<UrlOrRule>,
    /// 请求头(值支持 `{{name}}` 模板,便于带 `Authorization: Bearer {{token}}`)。
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// 本步响应上的有序命名捕获(按数组顺序求值;`capture[i]` 可引用 `capture[0..i]`)。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capture: Vec<Capture>,
    /// 惰性短路:列出的 key 在作用域内**全部非空**则跳过本步(token 复用,避免每章重抓)。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub skip_if_present: Vec<String>,
}

/// 单个请求。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct Request {
    pub url: UrlOrRule,
    #[serde(default)]
    pub method: Method,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<UrlOrRule>,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    /// 命名捕获:对**本请求响应**每个 `(name, Rule)` 求值,写入 `chapter` 层(等价 `scope=chapter`
    /// 的 [`Capture`]),使本 op 的 list/item 与后续步骤以 `{{name}}` 引用。见 design D7-bis。
    /// 各条**独立**对响应求值(可引用 `base`/`key`/`page` 与前置 `prelude` 捕获,但**勿互相引用**:
    /// 需有序依赖请用 `prelude` 链)。用 `BTreeMap` 保证迭代/落点顺序确定(免哈希随机化幽灵 bug)。
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub vars: BTreeMap<String, Rule>,
    /// 渲染型取页(`render-fetcher`):为真则用受控浏览器渲染本请求 URL、跑站点自身 JS,
    /// 而非 reqwest 直取——用于 SPA 站点(结果由 JS 渲染 / 签名 API 返回)。默认 false = 现状。
    /// 仅 `browser` feature 且浏览器可用时生效;否则该 op 优雅降级。
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub render: bool,
    /// 渲染就绪等待选择器。两种用法:
    /// - **无 `interceptApi`**:渲染后轮询该选择器出现,取渲染后 DOM 交 CSS 规则(方式 A);
    /// - **与 `interceptApi` 共存**(`render-dual-source`):拦 API 取 body 之外,等该选择器出现后
    ///   另抓渲染 DOM,供 `via:css` 的 `totalPages` 等对 DOM 求值(如分页器总页数)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ready_for: Option<String>,
    /// CDP 拦截:渲染时拦截 URL 含此子串的响应体作为取页 body(交 `via:"json"` 规则)。
    /// 用于「结果只在签名 API、DOM 无关键字段」的站点(典型:番茄搜索 `search_book/v1`)。
    /// 可与 `ready_for` 共存(见上)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub intercept_api: Option<String>,
    /// 精确总页数规则(`render-dual-source`):对取页结果求值得「共 M 页」(search 翻页进度)。
    /// `via:json` 对 API body 求值;`via:css`/`xpath` 对渲染 DOM 求值(需 `interceptApi` +
    /// `ready_for` 共存,抓到 DOM 才有源)。空 = 不返回总数(现状)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_pages: Option<Rule>,
    /// 翻页边界规则(`list-has-more`):对取页结果求值「是否还有下一页」(非空且非 `false`/`0` → 有)。
    /// 源路由同 `total_pages`(按 `via`)。空 = 不提供边界(UI 不限制)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub has_more: Option<Rule>,
}
