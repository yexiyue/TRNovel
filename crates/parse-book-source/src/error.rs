//! 分层错误(v2)。fetch/verify 各层错误在对应阶段实现时并入。

use thiserror::Error;

/// 书源 v2 顶层错误。
#[derive(Debug, Error)]
pub enum BookSourceError {
    /// 配置(书源 JSON)解析失败。
    #[error("book source config error: {0}")]
    Config(#[from] ConfigError),
    /// 规则求值失败。
    #[error("rule eval error: {0}")]
    Eval(#[from] EvalError),
    /// 取页失败。
    #[error("fetch error: {0}")]
    Fetch(#[from] FetchError),
    /// 操作所需的配置缺失(如未配置 search/explore)。
    #[error("book source missing config: {0}")]
    Missing(&'static str),
}

impl BookSourceError {
    /// 是否为「被反爬挑战拦截」(如 Cloudflare 托管挑战)。
    /// 用于诊断/降级:据此给出精确提示而非笼统失败,并决定是否升级浏览器取页。
    pub fn is_challenge(&self) -> bool {
        matches!(self, BookSourceError::Fetch(FetchError::Challenged(_)))
    }
}

/// 取页层错误。
#[derive(Debug, Error)]
pub enum FetchError {
    /// 网络/HTTP 错误。
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    /// 非法请求头。
    #[error("invalid header: {0}")]
    Header(String),
    /// 响应解码失败。
    #[error("decode error: {0}")]
    Decode(String),
    /// 被反爬挑战拦截(如 Cloudflare 托管挑战):拿到的是挑战页而非真实内容。
    #[error("blocked by anti-bot challenge: {0}")]
    Challenged(String),
    /// 浏览器解挑战失败(仅 `browser` feature)。
    #[cfg(feature = "browser")]
    #[error("browser solve error: {0}")]
    Browser(String),
}

/// 配置层错误。
#[derive(Debug, Error)]
pub enum ConfigError {
    /// 书源 JSON 反序列化失败。
    #[error("invalid book source json: {0}")]
    Json(#[from] serde_json::Error),
    /// 读取本地书源文件失败。
    #[error("read book source file: {0}")]
    Io(#[from] std::io::Error),
}

/// 规则求值层错误。
#[derive(Debug, Error)]
pub enum EvalError {
    /// CSS 选择器非法。
    #[error("invalid css selector: {0}")]
    Selector(String),
    /// 正则非法。
    #[error("invalid regex: {0}")]
    Regex(String),
    /// JSONPath 查询失败。
    #[error("jsonpath error: {0}")]
    JsonPath(String),
    /// XPath 表达式非法或求值失败。
    #[error("xpath error: {0}")]
    Xpath(String),
    /// 待解析内容不是合法 JSON。
    #[error("invalid json content: {0}")]
    Json(String),
    /// 该 via 后端暂未启用(如 xpath)。
    #[error("extraction backend not enabled: {0}")]
    Unsupported(&'static str),
}

/// v2 结果别名。
pub type Result<T> = std::result::Result<T, BookSourceError>;
