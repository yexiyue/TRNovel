//! v2 书源配置类型(纯 serde,镜像 `book-source.schema.json`)。
//!
//! 规则是显式结构化对象,无任何紧凑字符串 DSL。`Rule` 既是配置、也是供求值器
//! 遍历的语法树(见 design D1/D6)。

use super::error::ConfigError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

/// HTTP 方法。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Method {
    #[default]
    Get,
    Post,
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
    /// 命名捕获,供 template 使用。
    #[serde(default)]
    pub vars: HashMap<String, Rule>,
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

/// 搜索操作。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct SearchOp {
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchOp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explore: Option<ExploreOp>,
    pub book_info: BookRules,
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
