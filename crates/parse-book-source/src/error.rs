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
    /// 登录态失效(`loginCheckJs` 在响应期判定),需用户重新登录。
    #[error("登录态已失效,请重新登录")]
    LoginExpired,
}

impl BookSourceError {
    /// 是否为「被反爬挑战拦截」(如 Cloudflare 托管挑战)。
    /// 用于诊断/降级:据此给出精确提示而非笼统失败,并决定是否升级浏览器取页。
    pub fn is_challenge(&self) -> bool {
        matches!(self, BookSourceError::Fetch(FetchError::Challenged(_)))
    }

    /// 是否为「登录态失效」(`loginCheckJs` 判定):据此提示用户重新登录。
    pub fn is_login_expired(&self) -> bool {
        matches!(self, BookSourceError::LoginExpired)
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
    /// clean 编解码算子失败(非法 base64/hex/url 等)。
    #[error("codec error: {0}")]
    Codec(String),
    /// clean 加解密算子失败(密钥/IV 长度错、padding 错、密文损坏等)。
    #[error("crypto error: {0}")]
    Crypto(String),
    /// JS 脚本求值失败(语法错/运行错;仅 `js` feature)。
    #[error("js error: {0}")]
    Js(String),
    /// clean 字体反爬还原算子失败(未知内置表 / 非法码点 / 缺映射表)。
    #[error("font map error: {0}")]
    Font(String),
    /// JS host 桥失败(网络/cookie/登录态等;仅 `js-host` feature)。
    /// 以可被 JS `try/catch` 捕获的方式抛出,不使整段求值崩溃。
    #[cfg(feature = "js-host")]
    #[error("host bridge error: {0}")]
    Host(String),
}

/// v2 结果别名。
pub type Result<T> = std::result::Result<T, BookSourceError>;
