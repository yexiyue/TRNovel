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

    /// 解析 `concurrentRate` 字符串:`"N/ms"`(N 次每 ms)或纯毫秒间隔(`"1000"` = 每 1000ms 一次)。
    fn from_rate_str(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let (max_count, per_ms) = match s.split_once('/') {
            Some((n, ms)) => (n.trim().parse().ok()?, ms.trim().parse().ok()?),
            None => (1, s.parse().ok()?),
        };
        Self::from_config(&RateLimit { max_count, per_ms })
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

/// 判定一次响应是否为反爬挑战(纯函数,便于离线测试)。
///
/// 命中任一即视为挑战页(而非真实内容):
/// ① 响应头 `cf-mitigated: challenge`(最干净、机器可读);
/// ② HTTP 403/503 且 body 含 Cloudflare 挑战脚本特征
///    (`_cf_chl_opt` / `/cdn-cgi/challenge-platform/` / `<title>Just a moment`)。
pub fn is_challenge(status: u16, cf_mitigated: Option<&str>, body: &str) -> bool {
    if cf_mitigated == Some("challenge") {
        return true;
    }
    matches!(status, 403 | 503)
        && (body.contains("_cf_chl_opt")
            || body.contains("/cdn-cgi/challenge-platform/")
            || body.contains("<title>Just a moment"))
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

/// 一次取页的完整响应:解码后 body + HTTP 状态码 + 响应头。
///
/// 供 `net.connect` 读取 `Set-Cookie` / `Location` / 状态码等(`fetch` 只回 body)。
/// 同名多值头(如多个 `Set-Cookie`)以 `\n` 连接。
#[derive(Debug, Clone, Default)]
pub struct FetchResponse {
    pub body: String,
    pub status: u16,
    pub headers: HashMap<String, String>,
}

/// 取页抽象。实现者负责发请求 + 按目标站字符集解码为文本。
#[async_trait]
pub trait Fetcher: Send + Sync {
    /// 取一个页面的解码后文本。
    async fn fetch(&self, req: FetchRequest) -> Result<String, FetchError>;

    /// 取完整响应(body + 状态码 + 响应头)。默认仅回 body(状态 200、头为空);
    /// 需要 headers/状态码的实现(如 [`ReqwestFetcher`])应覆盖本方法。
    async fn fetch_full(&self, req: FetchRequest) -> Result<FetchResponse, FetchError> {
        let body = self.fetch(req).await?;
        Ok(FetchResponse {
            body,
            status: 200,
            headers: HashMap::new(),
        })
    }
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
            // 限速来源:优先 http.rateLimit,否则 concurrentRate("N/ms" 或间隔)。
            limiter: http
                .rate_limit
                .as_ref()
                .and_then(RateLimiter::from_config)
                .or_else(|| RateLimiter::from_rate_str(&source.concurrent_rate)),
        })
    }

    /// 发起一次请求并解码(单次,不含重试),返回完整响应(body + 状态码 + 响应头)。
    async fn send_once(&self, url: &str, req: &FetchRequest) -> Result<FetchResponse, FetchError> {
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
        let resp = builder.send().await?;
        let status = resp.status();
        // 收集响应头(同名多值以 `\n` 连接,保留多个 Set-Cookie)。
        let mut headers = HashMap::new();
        for (name, value) in resp.headers() {
            if let Ok(v) = value.to_str() {
                headers
                    .entry(name.as_str().to_string())
                    .and_modify(|e: &mut String| {
                        e.push('\n');
                        e.push_str(v);
                    })
                    .or_insert_with(|| v.to_string());
            }
        }
        // 反爬信号:`cf-mitigated: challenge` 头(最干净、机器可读)。
        let cf_mitigated = headers.get("cf-mitigated").map(String::as_str);
        // 先取出 HTTP 状态错误(error_for_status_ref 不消费 body),
        // 再读 body 以便识别挑战页特征(挑战常以 403 返回)。
        let status_err = resp.error_for_status_ref().err();
        let cf_mitigated = cf_mitigated.map(str::to_owned);
        let bytes = resp.bytes().await?;
        let text = self.decode(&bytes);
        if is_challenge(status.as_u16(), cf_mitigated.as_deref(), &text) {
            return Err(FetchError::Challenged(format!(
                "Cloudflare/反爬挑战 @ {url}"
            )));
        }
        if let Some(e) = status_err {
            return Err(FetchError::Http(e));
        }
        Ok(FetchResponse {
            body: text,
            status: status.as_u16(),
            headers,
        })
    }

    /// 限速 + resolve + 重试 的取页主循环,返回完整响应。
    async fn fetch_full_inner(&self, req: FetchRequest) -> Result<FetchResponse, FetchError> {
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
                Ok(resp) => return Ok(resp),
                Err(e) => {
                    // 反爬挑战重试无意义(仍会被挑战),直接返回交上层升级/降级。
                    if matches!(e, FetchError::Challenged(_)) || attempt >= max {
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

    /// 把相对路径解析为绝对 URL(`http(s)` 开头则原样返回)。
    pub(crate) fn resolve(&self, url: &str) -> String {
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
        self.fetch_full_inner(req).await.map(|r| r.body)
    }

    async fn fetch_full(&self, req: FetchRequest) -> Result<FetchResponse, FetchError> {
        self.fetch_full_inner(req).await
    }
}

#[cfg(test)]
mod tests {
    use super::is_challenge;

    /// Cloudflare 托管挑战页的最小特征(取自实测 bilixs 响应)。
    const CHALLENGE_HTML: &str = r#"<html><head><title>Just a moment...</title></head>
        <body><script>window._cf_chl_opt={cType:'managed'};
        a.src='/cdn-cgi/challenge-platform/h/g/orchestrate/chl_page/v1';</script></body></html>"#;

    const NORMAL_HTML: &str =
        r#"<html><head><title>蛊真人 搜索结果</title></head><body>正文</body></html>"#;

    #[test]
    fn cf_mitigated_header_is_challenge() {
        // 即便状态 200,带 cf-mitigated: challenge 头也判为挑战。
        assert!(is_challenge(200, Some("challenge"), NORMAL_HTML));
    }

    #[test]
    fn challenge_body_with_403_is_challenge() {
        assert!(is_challenge(403, None, CHALLENGE_HTML));
        assert!(is_challenge(503, None, CHALLENGE_HTML));
    }

    #[test]
    fn normal_200_page_is_not_challenge() {
        assert!(!is_challenge(200, None, NORMAL_HTML));
        // 仅有挑战特征但状态 200(无 cf-mitigated)不误判,避免正文含 cdn-cgi 字样被冤枉。
        assert!(!is_challenge(200, None, CHALLENGE_HTML));
    }

    #[test]
    fn challenge_markers_without_bad_status_not_challenge() {
        // 403 但 body 无挑战特征 → 不是挑战(交由普通 HTTP 错误处理)。
        assert!(!is_challenge(403, None, NORMAL_HTML));
    }
}
