//! 升级式取页装饰器 [`EscalatingFetcher`]:平时 reqwest,撞挑战/渲染时升级浏览器并缓存 clearance。

use super::*;

/// 升级式取页(装饰器 / 责任链,见 design D10):
/// 先走 reqwest;命中反爬挑战且浏览器可用 → 用 [`BrowserFetcher`] 解出 `cf_clearance`,
/// 注入后重试 reqwest;之后请求复用该 clearance(cookie 烤箱)。
pub struct EscalatingFetcher {
    reqwest: ReqwestFetcher,
    browser: Option<BrowserFetcher>,
    clearance: Mutex<Option<Clearance>>,
    /// 书源名,用于授权弹窗展示。
    name: String,
}

impl EscalatingFetcher {
    /// `browser` 为 `None` 时退化为纯 reqwest(撞挑战即返回 `Challenged` 由上层降级)。
    pub fn new(source: &BookSource, browser: Option<BrowserFetcher>) -> Result<Self, FetchError> {
        Ok(Self {
            reqwest: ReqwestFetcher::new(source)?,
            browser,
            clearance: Mutex::new(None),
            name: source.name.clone(),
        })
    }

    /// 把已有 clearance(cookie + 真实 UA)注入请求头。
    async fn apply_clearance(&self, req: &mut FetchRequest) {
        if let Some(c) = self.clearance.lock().await.as_ref() {
            req.headers
                .entry("Cookie".into())
                .or_insert_with(|| c.cookie_header.clone());
            // UA 必须与签发 cf_clearance 的浏览器一致(覆盖书源配置的 UA,见 design D6)。
            req.headers
                .insert("User-Agent".into(), c.user_agent.clone());
        }
    }
}

#[async_trait]
impl Fetcher for EscalatingFetcher {
    async fn fetch(&self, req: FetchRequest) -> Result<String, FetchError> {
        self.fetch_full(req).await.map(|r| r.body)
    }

    /// 升级式取完整响应:透传真实状态码与响应头(`net.connect` 依赖之),并保留解挑战逻辑。
    /// `fetch` 委托本方法 —— 升级语义实现一次,避免漂移。
    async fn fetch_full(&self, mut req: FetchRequest) -> Result<FetchResponse, FetchError> {
        // 渲染型取页(`render-fetcher`):用受控浏览器渲染本 URL、跑站点自身 JS(默认 headless),
        // 取「拦截的 API 响应」(优先)或「渲染后 DOM」。失败优雅降级(该 op 不可用,不影响其它)。
        if req.render {
            let Some(browser) = &self.browser else {
                return Err(FetchError::Challenged(format!(
                    "渲染取页需浏览器辅助,当前不可用 @ {}",
                    req.url
                )));
            };
            // 本会话渲染已判定不可用 → 直接降级,不再反复开浏览器频闪(与 SOLVE_FAILED 对称)。
            if RENDER_FAILED.load(Ordering::Relaxed) {
                return Err(FetchError::Challenged(format!(
                    "渲染取页:浏览器辅助本会话已停用、已降级(可重启 app 重试)@ {}",
                    req.url
                )));
            }
            let abs = self.reqwest.resolve(&req.url);
            // 渲染超时调小(12s):失败更快、不再像卡死;渲染/拦截类瞬态失败有界重试一次。
            let timeout = Duration::from_secs(12);
            let mut attempt = 0u32;
            let body = loop {
                let r = if let Some(api) = &req.intercept_api {
                    browser.render_intercept(&abs, api, timeout, true).await
                } else if let Some(ready) = &req.ready_for {
                    browser.render_dom(&abs, ready, timeout, true).await
                } else {
                    return Err(FetchError::Challenged(format!(
                        "render=true 需指定 interceptApi 或 readyFor @ {abs}"
                    )));
                };
                match r {
                    Ok(b) => break b,
                    // 启动类失败(浏览器不可用)→ 置会话熔断,直接降级、不重试。
                    Err(e @ FetchError::Browser(_)) => {
                        RENDER_FAILED.store(true, Ordering::Relaxed);
                        return Err(e);
                    }
                    // 渲染/拦截失败(超时/未拦截到,常为瞬态/风控)→ 有界重试一次
                    //(复用常驻浏览器、只重开新 Page,不再重启整个浏览器)。
                    Err(e) => {
                        if attempt >= RENDER_RETRY {
                            return Err(e);
                        }
                        attempt += 1;
                        tokio::time::sleep(Duration::from_millis(1000)).await;
                    }
                }
            };
            return Ok(FetchResponse {
                body,
                status: 200,
                ..Default::default()
            });
        }
        self.apply_clearance(&mut req).await;
        match self.reqwest.fetch_full(req.clone()).await {
            Err(FetchError::Challenged(msg)) => {
                let Some(browser) = &self.browser else {
                    return Err(FetchError::Challenged(msg));
                };
                // 本会话已判定浏览器不可用 → 直接降级,不再启动(避免频闪)。
                if SOLVE_FAILED.load(Ordering::Relaxed) {
                    return Err(FetchError::Challenged(format!(
                        "{msg}(浏览器辅助不可用,已降级;可重启 app 重试)"
                    )));
                }
                // 串行化解挑战:持锁期间若并发的其它取页已解出 clearance,直接复用,
                // 避免重复开浏览器 / 重复弹授权窗。
                let mut guard = self.clearance.lock().await;
                if guard.is_none() {
                    // 升级前征求用户授权(若提供了 UI);拒绝则降级。
                    if let Some(ui) = browser.ui()
                        && ui.authorize(&self.name).await == AuthDecision::Deny
                    {
                        return Err(FetchError::Challenged(format!(
                            "{msg}(用户未授权浏览器辅助)"
                        )));
                    }
                    let abs = self.reqwest.resolve(&req.url);
                    match browser.solve(&abs).await {
                        Ok(c) => *guard = Some(c),
                        Err(e) => {
                            // 启动/解挑战失败:本会话停用浏览器辅助,避免反复重试导致频闪。
                            SOLVE_FAILED.store(true, Ordering::Relaxed);
                            return Err(e);
                        }
                    }
                }
                drop(guard);
                self.apply_clearance(&mut req).await;
                self.reqwest.fetch_full(req).await
            }
            other => other,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{BrowserCookie, EscalatingFetcher, LoginOutcome, detect_browser};
    use crate::fetch::{FetchRequest, Fetcher};
    use crate::testutil::{book_source, spawn_fixed_server};

    #[test]
    fn detect_browser_does_not_panic() {
        // 探测不应 panic;有无浏览器取决于运行机器,这里只验证可调用。
        let _ = detect_browser();
    }

    // ── 7.x:登录产出按注册域归并 cookie(纯逻辑,无需真浏览器)──
    #[test]
    fn login_outcome_groups_cookies_by_registrable_domain() {
        let out = LoginOutcome {
            cookies: vec![
                BrowserCookie {
                    domain: ".www.site.com".into(),
                    name: "sid".into(),
                    value: "1".into(),
                },
                BrowserCookie {
                    domain: "api.site.com".into(),
                    name: "t".into(),
                    value: "2".into(),
                },
                BrowserCookie {
                    domain: "a.example.co.uk".into(),
                    name: "x".into(),
                    value: "9".into(),
                },
            ],
            ..Default::default()
        };
        let by = out.cookies_by_registrable_domain();
        // 子域按二级域 site.com 归并、键有序拼接。
        assert_eq!(by.get("site.com").map(String::as_str), Some("sid=1; t=2"));
        // 公共后缀 co.uk → 注册域 example.co.uk。
        assert_eq!(by.get("example.co.uk").map(String::as_str), Some("x=9"));
    }

    // ── 审查/correctness:EscalatingFetcher 必须覆盖 fetch_full,透传真实状态码与响应头 ──
    // 否则落到默认实现 → net.connect 静默退化为 {code:200, headers:{}},打掉登录脚本读 Set-Cookie 的能力。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn escalating_fetcher_fetch_full_passes_status_and_headers() {
        let (base, server) = spawn_fixed_server(
            "HTTP/1.1 201 Created\r\nSet-Cookie: sid=zzz; Path=/\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                .to_string(),
        );
        // browser=None:不解挑战,但 fetch_full 仍须透传 reqwest 的真实 status/headers。
        let fetcher = EscalatingFetcher::new(&book_source(&base), None).unwrap();
        let resp = fetcher.fetch_full(FetchRequest::get("/x")).await.unwrap();
        server.join().unwrap();
        assert_eq!(resp.status, 201, "应透传真实状态码,而非默认 200");
        assert_eq!(
            resp.headers.get("set-cookie").map(String::as_str),
            Some("sid=zzz; Path=/"),
            "应透传响应头(Set-Cookie),而非默认空 headers"
        );
    }

    // ── render-fetcher:无浏览器时渲染请求优雅降级(不 panic、不卡死,给含「浏览器」的提示)──
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn render_request_degrades_without_browser() {
        use crate::error::FetchError;
        let fetcher = EscalatingFetcher::new(&book_source("https://e.com"), None).unwrap();
        let req = FetchRequest {
            url: "/search/x".into(),
            render: true,
            intercept_api: Some("search_book/v1".into()),
            ..Default::default()
        };
        let err = fetcher.fetch_full(req).await.unwrap_err();
        assert!(
            matches!(err, FetchError::Challenged(_)) && err.to_string().contains("浏览器"),
            "无浏览器的 render 请求应降级为含「浏览器」的 Challenged,实际: {err}"
        );
    }
}
