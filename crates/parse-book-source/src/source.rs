//! v2 书源配置类型(纯 serde,镜像 `book-source.schema.json`)。
//!
//! 规则是显式结构化对象,无任何紧凑字符串 DSL。`Rule` 既是配置、也是供求值器
//! 遍历的语法树(见 design D1/D6)。

use super::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

// ───────────────────────── 规则 AST ─────────────────────────

/// 抽取后端(决定 `select` 的语义)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Via {
    #[default]
    Css,
    Xpath,
    Json,
    Regex,
    /// 直接使用当前上下文值(只跑 clean)。
    Raw,
}

/// 取值方式(枚举字符串 或 `{ "attr": "..." }`)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(untagged)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Extract {
    Op(ExtractOp),
    Attr { attr: String },
}

impl Default for Extract {
    fn default() -> Self {
        Extract::Op(ExtractOp::Text)
    }
}

/// 文本/HTML 取值算子。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ExtractOp {
    #[default]
    Text,
    OwnText,
    Html,
    InnerHtml,
    OuterHtml,
}

/// 编解码方式(`decode`/`encode` 算子,以及 crypto 的字节↔串编码)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Codec {
    Base64,
    Base64url,
    Hex,
    /// URL 百分号编解码。
    Url,
}

/// crypto 的 key/iv/输入/输出字节编码。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ByteEnc {
    #[default]
    Utf8,
    Base64,
    Hex,
    /// 原样字节(等同 utf8 字节,主要用于输入密文已是裸字节串的场景)。
    Raw,
}

/// 哈希算法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum HashAlgo {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

/// 哈希/HMAC 输出编码。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum HashOut {
    #[default]
    Hex,
    Base64,
}

/// 哈希算子(可选 HMAC)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HashStep {
    pub algo: HashAlgo,
    #[serde(default)]
    pub output: HashOut,
    /// 提供则计算 HMAC(以此为密钥)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hmac_key: Option<String>,
    #[serde(default)]
    pub hmac_key_enc: ByteEnc,
}

/// 对称加密算法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum CipherAlgo {
    Aes,
    Des,
    TripleDes,
}

/// 加密模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum CipherMode {
    Cbc,
    Ecb,
    Cfb,
    Gcm,
}

/// 填充方式(gcm 忽略)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Padding {
    #[default]
    Pkcs7,
    Zero,
    None,
}

/// 加解密方向。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum CipherOp {
    #[default]
    Decrypt,
    Encrypt,
}

/// 加解密算子。默认值贴合「解密正文」主场景:`op=decrypt`、`inputEnc=base64`、`outputEnc=utf8`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CipherStep {
    pub algo: CipherAlgo,
    pub mode: CipherMode,
    #[serde(default)]
    pub padding: Padding,
    #[serde(default)]
    pub op: CipherOp,
    pub key: String,
    #[serde(default)]
    pub key_enc: ByteEnc,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iv: Option<String>,
    #[serde(default)]
    pub iv_enc: ByteEnc,
    /// 入参密文串→字节;省略时按 `op` 取默认(decrypt→base64,encrypt→utf8)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_enc: Option<ByteEnc>,
    /// 结果字节→串;省略时按 `op` 取默认(decrypt→utf8,encrypt→base64)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_enc: Option<ByteEnc>,
}

/// 繁简转换方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum CnConvert {
    /// 繁体 → 简体。
    T2s,
    /// 简体 → 繁体。
    S2t,
}

/// 单步后处理。步内多算子按固定顺序执行:
/// `regex/replace → trim → prepend → append → decode → encode → hash → cipher → fontMap → cn`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CleanStep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append: Option<String>,
    /// 解码(base64/base64url/hex/url)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decode: Option<Codec>,
    /// 编码(base64/base64url/hex/url)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode: Option<Codec>,
    /// 哈希/HMAC。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<HashStep>,
    /// 对称加解密。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cipher: Option<CipherStep>,
    /// 字体反爬还原:私有区(PUA)字符按映射表换回真字。键为码点十六进制(如 `"E4DE"` 或 `"U+E4DE"`),
    /// 值为目标字符;表外字符原样保留。用于番茄等「自定义字体 + PUA」反爬站点——表是数据,由书源内联
    /// 提供(引擎不内置任何站点的表),可用 `trn gen-fontmap` 生成。
    #[serde(default, rename = "fontMap", skip_serializing_if = "Option::is_none")]
    pub font_map: Option<std::collections::BTreeMap<String, String>>,
    /// 繁简转换。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cn: Option<CnConvert>,
    /// JS 后处理(逃生舱;脚本里以当前串为 `result`)。需启用 `js` feature。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub js: Option<String>,
}

/// 叶子规则:在当前上下文做一次抽取。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LeafRule {
    #[serde(default)]
    pub via: Via,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<i64>,
    #[serde(default)]
    pub extract: Extract,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clean: Vec<CleanStep>,
}

/// 一条规则:叶子,或组合子。组合子按其唯一键判别(见 design D1)。
///
/// 反序列化时按变体顺序尝试:组合子(各有唯一必填键)在前,叶子兜底。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(untagged)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Rule {
    /// 取首个非空子规则结果(回退/自愈)。
    FirstOf {
        #[serde(rename = "firstOf")]
        first_of: Vec<Rule>,
    },
    /// 拼接非空子规则结果。
    Concat {
        concat: Vec<Rule>,
        #[serde(default)]
        join: String,
    },
    /// 字面量。
    Literal { literal: String },
    /// 模板插值(`{{key}}`/`{{page}}`/命名变量)。
    Template { template: String },
    /// JS 逻辑编排逃生舱(值规则):以当前上下文为 `result`、注入 `baseUrl`/变量 + `crypto`
    /// 助手求值,返回字符串。求值需启用 `js` feature(否则返回 `Unsupported("js")`)。
    /// 必须置于 `Leaf` 之前——`js` 是其唯一判别键,否则会被全可选的 `Leaf` 吞掉。
    Js { js: String },
    /// 叶子(兜底)。
    Leaf(LeafRule),
}

/// URL 字段:可为字符串模板,或一条规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(untagged)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum UrlOrRule {
    Str(String),
    Rule(Box<Rule>),
}

// ───────────────────────── HTTP / 请求 ─────────────────────────

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
}

// ───────────────────────── 操作规则 ─────────────────────────

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
#[serde(deny_unknown_fields)]
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
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct ExploreOp {
    /// 分类请求之前按序执行的前置请求链(见 design D7-bis);空 = 现状。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub prelude: Vec<PreStep>,
    pub categories: Vec<Category>,
    pub list: Rule,
    pub item: BookRules,
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

// ───────────────────────── 样例 ─────────────────────────

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

// ───────────────────────── 顶层书源 ─────────────────────────

/// v2 书源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BookSource {
    /// 固定为 `"trnovel-booksource/v2"`。
    pub schema: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    /// 站点基址,用于相对链接解析与 `{{base}}`。
    pub url: String,
    #[serde(default)]
    pub http: Http,
    /// 登录:普通 URL,或登录脚本(`@js:…` / `<js>…</js>` 包裹,内含 `login()` 函数)。
    /// 非空即视为「需要登录」(见 [`BookSource::has_login`]);仅 `js-host` 构建可真正执行脚本登录。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub login_url: String,
    /// 声明式登录表单(TUI 渲染);收集值加密存为 loginInfo,供 `login()` 脚本读取。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub login_ui: Vec<RowUi>,
    /// 登录态过期校验脚本:每个网络方法响应后执行(注入 `result`=响应),判失效可提示重登。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub login_check_js: String,
    /// 开启后:响应的 `Set-Cookie` 自动回灌进 cookie 库(按注册域归并持久化)。
    #[serde(default)]
    pub enabled_cookie_jar: bool,
    /// 限速:`"N/ms"`(N 次/ms)或纯毫秒间隔字符串;为空则用 `http.rateLimit`。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub concurrent_rate: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchOp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explore: Option<ExploreOp>,
    pub book_info: BookInfoOp,
    pub toc: TocRules,
    pub content: ContentRules,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<Sample>,
}

/// 期望的 schema 标识。
pub const SCHEMA_ID: &str = "trnovel-booksource/v2";

impl BookSource {
    /// 从 JSON 字符串解析一个书源。
    pub fn from_json(s: &str) -> Result<Self, ConfigError> {
        Ok(serde_json::from_str(s)?)
    }

    /// 是否需要登录(`loginUrl` 或 `loginUi` 任一非空)。据此在 TUI 暴露「书源登录」入口。
    /// 注:仅配置 `loginUi` 而无登录脚本/`loginUrl` 属配置不完整,由登录页拦截并提示。
    pub fn has_login(&self) -> bool {
        !self.login_url.trim().is_empty() || !self.login_ui.is_empty()
    }

    /// 若 `loginUrl` 是登录脚本(`@js:` 或 `<js>…</js>` 包裹),剥壳返回脚本体;
    /// 否则(普通 URL 或空)返回 `None`——此时走 headful 浏览器登录。
    pub fn get_login_js(&self) -> Option<&str> {
        let s = self.login_url.trim();
        if let Some(js) = s.strip_prefix("@js:") {
            Some(js.trim())
        } else if let Some(rest) = s.strip_prefix("<js>") {
            Some(rest.strip_suffix("</js>").unwrap_or(rest).trim())
        } else {
            None
        }
    }

    /// 从 JSON 值解析一个或多个书源(支持单对象或数组)。
    pub fn from_value_many(value: serde_json::Value) -> Result<Vec<Self>, ConfigError> {
        if value.is_array() {
            Ok(serde_json::from_value(value)?)
        } else {
            Ok(vec![serde_json::from_value(value)?])
        }
    }

    /// 从本地文件导入(支持单对象或数组)。
    pub fn from_path(path: &str) -> Result<Vec<Self>, super::error::BookSourceError> {
        let text = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
        let value = serde_json::from_str(&text).map_err(ConfigError::Json)?;
        Ok(Self::from_value_many(value)?)
    }

    /// 从网络 URL 导入(支持单对象或数组)。
    pub async fn from_url(url: &str) -> Result<Vec<Self>, super::error::BookSourceError> {
        use super::error::FetchError;
        let text = reqwest::get(url)
            .await
            .map_err(FetchError::Http)?
            // 先判 HTTP 状态:4xx/5xx 返回错误页时,避免把"非 JSON"误报成 JSON 解析失败。
            .error_for_status()
            .map_err(FetchError::Http)?
            .text()
            .await
            .map_err(FetchError::Http)?;
        let value = serde_json::from_str(&text).map_err(ConfigError::Json)?;
        Ok(Self::from_value_many(value)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 与 examples/bilixs.v2.json 同构的代表性书源(覆盖 leaf / firstOf / concat /
    /// template / attr / http+cookies / samples / 分卷 isVolume)。
    const BILIXS_V2: &str = r#"{
      "schema": "trnovel-booksource/v2",
      "name": "哔哩小说",
      "group": "测试",
      "url": "https://www.bilixs.com",
      "http": {
        "headers": { "User-Agent": "Mozilla/5.0" },
        "cookies": {},
        "warmup": ["https://www.bilixs.com/"],
        "charset": "auto",
        "timeout": 15000,
        "retry": { "max": 2, "backoffMs": 500 }
      },
      "search": {
        "request": { "url": { "template": "{{base}}/search.html?searchkey={{key}}" }, "method": "GET" },
        "list": { "via": "css", "select": ".module-item" },
        "item": {
          "name": { "via": "css", "select": ".module-item-title", "extract": "text" },
          "tocUrl": { "via": "css", "select": ".module-item-title", "extract": { "attr": "href" } }
        }
      },
      "explore": {
        "categories": [ { "title": "最近更新", "url": { "template": "{{base}}/book/lastupdate_0_1_0_0_0_0_0_{{page}}_0.html" } } ],
        "list": { "via": "css", "select": ".module-item" },
        "item": { "name": { "via": "css", "select": ".module-item-title", "extract": "text" } }
      },
      "bookInfo": {
        "name": { "via": "css", "select": "[property=\"og:novel:book_name\"]", "extract": { "attr": "content" } },
        "cover": { "via": "css", "select": "[property=\"og:image\"]", "extract": { "attr": "content" } },
        "kind": { "concat": [
            { "via": "css", "select": "[property=\"og:novel:tags\"]", "extract": { "attr": "content" } },
            { "via": "css", "select": "[property=\"og:novel:status\"]", "extract": { "attr": "content" } }
          ], "join": " · " },
        "tocUrl": { "via": "css", "select": "[property=\"og:novel:read_url\"]", "extract": { "attr": "content" } }
      },
      "toc": {
        "list": { "via": "css", "select": ".box > h2.module-title.type, .box a.module-row-text" },
        "name": { "firstOf": [
            { "via": "css", "select": ".module-row-title", "extract": "text" },
            { "via": "css", "select": "h2", "extract": "text" }
          ] },
        "url": { "via": "css", "select": "a", "extract": { "attr": "href" } },
        "isVolume": { "via": "css", "select": "h2", "extract": "text" },
        "maxPages": 1
      },
      "content": {
        "value": { "via": "css", "select": ".article-content", "extract": "html",
          "clean": [ { "regex": "请收藏本站[^<\\n]*", "replace": "" }, { "trim": true } ] }
      },
      "samples": [
        { "bookUrl": "/novel/guzhenren.html", "expect": { "name": "蛊真人", "volumes": 8, "minChapters": 2000 } }
      ]
    }"#;

    #[test]
    fn parses_v2_book_source() {
        let bs = BookSource::from_json(BILIXS_V2).expect("应解析 v2 书源");
        assert_eq!(bs.schema, SCHEMA_ID);
        assert_eq!(bs.name, "哔哩小说");
    }

    #[test]
    fn toc_name_is_firstof_with_two_leaves() {
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        match &bs.toc.name {
            Rule::FirstOf { first_of } => assert_eq!(first_of.len(), 2),
            other => panic!("toc.name 应为 firstOf,实际 {other:?}"),
        }
    }

    #[test]
    fn toc_is_volume_is_leaf_css_h2() {
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        let iv = bs.toc.is_volume.as_ref().expect("isVolume 应存在");
        match iv {
            Rule::Leaf(l) => {
                assert_eq!(l.via, Via::Css);
                assert_eq!(l.select.as_deref(), Some("h2"));
            }
            other => panic!("isVolume 应为叶子,实际 {other:?}"),
        }
    }

    #[test]
    fn search_url_is_template_rule() {
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        let req = &bs.search.as_ref().unwrap().request;
        match &req.url {
            UrlOrRule::Rule(r) => assert!(matches!(**r, Rule::Template { .. })),
            other => panic!("search.request.url 应为模板规则,实际 {other:?}"),
        }
    }

    #[test]
    fn book_info_cover_extracts_attr() {
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        match bs.book_info.cover.as_ref().unwrap() {
            Rule::Leaf(l) => assert_eq!(
                l.extract,
                Extract::Attr {
                    attr: "content".into()
                }
            ),
            other => panic!("cover 应为属性抽取叶子,实际 {other:?}"),
        }
    }

    #[test]
    fn http_cookies_and_warmup_parsed() {
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        assert_eq!(bs.http.warmup, vec!["https://www.bilixs.com/"]);
        assert_eq!(bs.http.charset, Charset::Auto);
        assert_eq!(bs.http.retry.as_ref().unwrap().backoff_ms, 500);
    }

    #[test]
    fn sample_expectations_parsed() {
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        let s = &bs.samples[0];
        assert_eq!(s.expect.volumes, Some(8));
        assert_eq!(s.expect.min_chapters, Some(2000));
    }

    #[test]
    fn round_trips_through_json() {
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        let json = serde_json::to_string(&bs).unwrap();
        let bs2 = BookSource::from_json(&json).unwrap();
        assert_eq!(bs, bs2);
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let bad = BILIXS_V2.replacen("\"name\":", "\"nmae\":", 1);
        assert!(
            BookSource::from_json(&bad).is_err(),
            "拼错字段应被 deny_unknown_fields 拒绝"
        );
    }

    // ── 审查/test-coverage:has_login 判定(决定 TUI 是否显示登录入口)──
    #[test]
    fn has_login_when_login_url_or_login_ui_present() {
        let mut bs = BookSource::from_json(BILIXS_V2).unwrap();
        assert!(!bs.has_login(), "默认无 loginUrl/loginUi 不需登录");
        bs.login_url = "https://site/login".into();
        assert!(bs.has_login());
        bs.login_url = "@js:function login(){}".into();
        assert!(bs.has_login());
        bs.login_url = "   ".into();
        assert!(!bs.has_login(), "纯空白 loginUrl 视为不需登录");
        // 仅配置 loginUi 也计入(TUI 须给登录入口;配置完整性由登录页校验)。
        bs.login_ui = vec![RowUi {
            name: "用户名".into(),
            ..Default::default()
        }];
        assert!(bs.has_login(), "loginUi 非空应计入登录入口判定");
    }

    // ── 审查/test-coverage:get_login_js 剥壳各分支(@js: / <js>…</js> / 半包裹 / 普通 URL / 空)──
    #[test]
    fn get_login_js_strips_prefixes() {
        let mut bs = BookSource::from_json(BILIXS_V2).unwrap();
        bs.login_url = "@js: function login(){} ".into();
        assert_eq!(bs.get_login_js(), Some("function login(){}"));
        bs.login_url = "<js>BODY</js>".into();
        assert_eq!(bs.get_login_js(), Some("BODY"));
        bs.login_url = "<js> A </js>".into();
        assert_eq!(bs.get_login_js(), Some("A"));
        bs.login_url = "<js>BODY".into(); // 缺尾标签:容错保留整段
        assert_eq!(bs.get_login_js(), Some("BODY"));
        bs.login_url = "https://site/login".into(); // 普通 URL → None(走浏览器登录)
        assert_eq!(bs.get_login_js(), None);
        bs.login_url = "".into();
        assert_eq!(bs.get_login_js(), None);
    }

    // ── 11.1:前置请求链 + 结构化捕获 解析 / round-trip / deny_unknown_fields ──
    #[test]
    fn parses_prelude_capture_and_round_trips() {
        let json = r#"{
          "schema":"trnovel-booksource/v2","name":"t","url":"https://x",
          "search":{
            "prelude":[{
              "url":{"template":"{{base}}/prepare"},
              "capture":[{"name":"token","value":{"via":"json","select":"$.token"},"scope":"source"}],
              "skipIfPresent":["token"]
            }],
            "request":{"url":{"template":"{{base}}/s?token={{token}}"}},
            "list":{"via":"css","select":".i"},
            "item":{"name":{"via":"css","select":".t","extract":"text"}}
          },
          "bookInfo":{"prelude":[{"url":{"template":"{{base}}/p"},"capture":[{"name":"csrf","value":{"via":"raw"}}]}]},
          "toc":{"list":{"via":"css","select":"a"},"name":{"via":"css","select":"a"},"url":{"via":"css","select":"a","extract":{"attr":"href"}}},
          "content":{"value":{"via":"css","select":".c"}}
        }"#;
        let bs = BookSource::from_json(json).expect("应解析含 prelude 的书源");
        let sp = &bs.search.as_ref().unwrap().prelude;
        assert_eq!(sp.len(), 1);
        assert_eq!(sp[0].capture[0].name, "token");
        assert_eq!(sp[0].capture[0].scope, VarScope::Source);
        assert_eq!(sp[0].skip_if_present, vec!["token".to_string()]);
        // bookInfo 前置步骤默认 scope = chapter。
        assert_eq!(bs.book_info.prelude[0].capture[0].scope, VarScope::Chapter);
        // round-trip 相等。
        let s = serde_json::to_string(&bs).unwrap();
        assert_eq!(BookSource::from_json(&s).unwrap(), bs);
    }

    #[test]
    fn prestep_rejects_unknown_field() {
        let bad = r#"{
          "schema":"trnovel-booksource/v2","name":"t","url":"https://x",
          "toc":{"prelude":[{"url":{"template":"{{base}}/p"},"captuer":[]}],
                 "list":{"via":"css","select":"a"},"name":{"via":"css","select":"a"},"url":{"via":"css","select":"a"}},
          "bookInfo":{},
          "content":{"value":{"via":"css","select":".c"}}
        }"#;
        assert!(
            BookSource::from_json(bad).is_err(),
            "PreStep 拼错字段(captuer)应被 deny_unknown_fields 拒"
        );
    }

    #[test]
    fn existing_source_serializes_without_new_fields() {
        // 向后兼容:无 prelude/vars 的书源序列化输出不含任何新字段(逐字节)。
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        let json = serde_json::to_string(&bs).unwrap();
        assert!(!json.contains("prelude"), "无前置链不应序列化 prelude");
        assert!(!json.contains("\"vars\""), "空 vars 不应序列化");
        assert!(!json.contains("skipIfPresent"));
        assert!(!json.contains("\"capture\""));
    }
}

/// 防漂移:`book-source.schema.json` 必须等于从类型现生成的 schema(`--features schema`)。
/// 失败说明改了配置类型却没重新生成 schema——按提示重跑 gen_schema 即可。
#[cfg(all(test, feature = "schema"))]
mod schema_sync {
    #[test]
    fn schema_is_in_sync() {
        let generated =
            serde_json::to_string_pretty(&schemars::schema_for!(crate::BookSource)).unwrap();
        let committed = include_str!("../book-source.schema.json");
        assert_eq!(
            generated.trim(),
            committed.trim(),
            "book-source.schema.json 与配置类型不同步;请重新生成:\n  \
             cargo run -p parse-book-source --features schema --example gen_schema \
             > crates/parse-book-source/book-source.schema.json"
        );
    }
}
