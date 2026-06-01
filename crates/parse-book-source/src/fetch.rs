//! 取页端口(Ports & Adapters)。`trait Fetcher` 抽象「拿一个 URL 的解码后正文」,
//! 默认实现 [`ReqwestFetcher`];反爬后端(wreq / FlareSolverr)可作为另一个 `Fetcher`
//! 适配器接入而不动引擎(见 design D8/D10)。

use super::error::FetchError;
use super::source::{BookSource, Charset, Method, RateLimit, Retry};
use async_trait::async_trait;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// 简单令牌桶式限速器:保证两次请求间隔 >= `interval`。
/// 锁仅用于读改时间戳,在 `.await` 前释放(不跨 await 持锁,符合 design D10)。
struct RateLimiter {
    interval: Duration,
    last: Mutex<Option<Instant>>,
}

impl RateLimiter {
    fn from_config(rl: &RateLimit) -> Option<Self> {
        if rl.max_count == 0 || rl.per_ms == 0 {
            return None;
        }
        Some(Self {
            interval: Duration::from_millis(rl.per_ms / rl.max_count),
            last: Mutex::new(None),
        })
    }

    async fn acquire(&self) {
        let wait = {
            let mut last = self.last.lock().expect("rate limiter mutex poisoned");
            let now = Instant::now();
            let wait = match *last {
                Some(prev) => self
                    .interval
                    .checked_sub(now.duration_since(prev))
                    .unwrap_or(Duration::ZERO),
                None => Duration::ZERO,
            };
            // 预约下一次可用时刻(即便本次需等待),使并发请求依次错开。
            *last = Some(now + wait);
            wait
        }; // 锁在此释放,sleep 不持锁
        if !wait.is_zero() {
            tokio::time::sleep(wait).await;
        }
    }
}

/// 一次取页请求(URL 已是最终待请求地址或相对路径)。
#[derive(Debug, Clone, Default)]
pub struct FetchRequest {
    pub url: String,
    pub method: Method,
    pub body: Option<String>,
    pub headers: HashMap<String, String>,
}

impl FetchRequest {
    /// 便捷构造一个 GET 请求。
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            ..Default::default()
        }
    }
}

/// 取页抽象。实现者负责发请求 + 按目标站字符集解码为文本。
#[async_trait]
pub trait Fetcher: Send + Sync {
    /// 取一个页面的解码后文本。
    async fn fetch(&self, req: FetchRequest) -> Result<String, FetchError>;
}

/// 基于 reqwest + rustls + cookie_store 的默认取页实现(含限速与重试)。
pub struct ReqwestFetcher {
    client: reqwest::Client,
    base: String,
    charset: Charset,
    retry: Option<Retry>,
    limiter: Option<RateLimiter>,
}

impl ReqwestFetcher {
    /// 依据书源的 `http` 配置构建客户端(默认头、静态 cookie、超时)。
    pub fn new(source: &BookSource) -> Result<Self, FetchError> {
        let http = &source.http;
        let mut headers = HeaderMap::new();
        for (k, v) in &http.headers {
            let name = HeaderName::from_bytes(k.as_bytes())
                .map_err(|e| FetchError::Header(e.to_string()))?;
            let val = HeaderValue::from_str(v).map_err(|e| FetchError::Header(e.to_string()))?;
            headers.insert(name, val);
        }
        // 静态 cookie 合成为 Cookie 头(会话 cookie 仍由 cookie_store 自动累积)。
        if !http.cookies.is_empty() {
            let cookie = http
                .cookies
                .iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("; ");
            let val =
                HeaderValue::from_str(&cookie).map_err(|e| FetchError::Header(e.to_string()))?;
            headers.insert(reqwest::header::COOKIE, val);
        }

        let mut builder = reqwest::Client::builder()
            .cookie_store(true)
            .default_headers(headers);
        if let Some(ms) = http.timeout {
            builder = builder.timeout(Duration::from_millis(ms));
        }
        let client = builder.build()?;

        Ok(Self {
            client,
            base: source.url.trim_end_matches('/').to_string(),
            charset: http.charset,
            retry: http.retry.clone(),
            limiter: http.rate_limit.as_ref().and_then(RateLimiter::from_config),
        })
    }

    /// 发起一次请求并解码(单次,不含重试)。
    async fn send_once(&self, url: &str, req: &FetchRequest) -> Result<String, FetchError> {
        let mut builder = match req.method {
            Method::Get => self.client.get(url),
            Method::Post => self.client.post(url),
        };
        for (k, v) in &req.headers {
            builder = builder.header(k, v);
        }
        if let Some(body) = &req.body {
            builder = builder.body(body.clone());
        }
        let resp = builder.send().await?.error_for_status()?;
        let bytes = resp.bytes().await?;
        Ok(self.decode(&bytes))
    }

    /// 把相对路径解析为绝对 URL(`http(s)` 开头则原样返回)。
    fn resolve(&self, url: &str) -> String {
        if url.starts_with("http://") || url.starts_with("https://") {
            url.to_string()
        } else if let Some(rest) = url.strip_prefix('/') {
            format!("{}/{}", self.base, rest)
        } else {
            format!("{}/{}", self.base, url)
        }
    }

    /// 按 charset 把响应字节解码为文本。
    fn decode(&self, bytes: &[u8]) -> String {
        use encoding_rs::{BIG5, GB18030, GBK, UTF_8};
        match self.charset {
            Charset::Utf8 => UTF_8.decode(bytes).0.into_owned(),
            Charset::Gbk => GBK.decode(bytes).0.into_owned(),
            Charset::Gb18030 => GB18030.decode(bytes).0.into_owned(),
            Charset::Big5 => BIG5.decode(bytes).0.into_owned(),
            Charset::Auto => {
                let (text, _, had_err) = UTF_8.decode(bytes);
                if had_err {
                    GBK.decode(bytes).0.into_owned()
                } else {
                    text.into_owned()
                }
            }
        }
    }
}

#[async_trait]
impl Fetcher for ReqwestFetcher {
    async fn fetch(&self, req: FetchRequest) -> Result<String, FetchError> {
        // 限速(如配置):错开请求间隔。
        if let Some(limiter) = &self.limiter {
            limiter.acquire().await;
        }
        let url = self.resolve(&req.url);

        // 重试:失败后按 backoff 退避,最多重试 retry.max 次。
        let max = self.retry.as_ref().map(|r| r.max).unwrap_or(0);
        let backoff = self.retry.as_ref().map(|r| r.backoff_ms).unwrap_or(0);
        let mut attempt = 0u32;
        loop {
            match self.send_once(&url, &req).await {
                Ok(text) => return Ok(text),
                Err(e) => {
                    if attempt >= max {
                        return Err(e);
                    }
                    attempt += 1;
                    if backoff > 0 {
                        tokio::time::sleep(Duration::from_millis(backoff)).await;
                    }
                }
            }
        }
    }
}
