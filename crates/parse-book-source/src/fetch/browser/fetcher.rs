//! 浏览器取页器 [`BrowserFetcher`]:headful 解 Cloudflare 挑战、渲染取页(DOM 轮询 / CDP 拦截)、手动登录。

use super::*;

/// 基于系统浏览器的解挑战器(cookie 烤箱)。
pub struct BrowserFetcher {
    exe: PathBuf,
    opts: BrowserOptions,
}

impl BrowserFetcher {
    /// 探测系统浏览器并构建;无可用浏览器返回 `None`。
    pub fn detect(opts: BrowserOptions) -> Option<Self> {
        detect_browser().map(|exe| Self { exe, opts })
    }

    /// 已知浏览器路径时直接构建。
    pub fn with_executable(exe: PathBuf, opts: BrowserOptions) -> Self {
        Self { exe, opts }
    }

    /// 交互 UI 回调(供上层在升级前征求授权)。
    pub fn ui(&self) -> Option<&Arc<dyn BrowserUi>> {
        self.opts.ui.as_ref()
    }

    /// headful 打开 `url` 解挑战,轮询取得 `cf_clearance`,返回可注入 reqwest 的 [`Clearance`]。
    pub async fn solve(&self, url: &str) -> Result<Clearance, FetchError> {
        // 经 launch 统一启动:取 BROWSER_LOCK 串行化(与 render/login 互斥同一 profile)+ 清单例锁。
        // 必须 headful(headless 解不开,见 design D4)。`_guard` 持到本函数末尾 close 之后。
        let (mut browser, handler_task, _guard) = self.launch(false).await?;

        let result = self.solve_inner(&browser, url).await;

        // 生命周期:无论成败都关闭浏览器、回收事件循环(D11),并撤下交互提示。
        let _ = browser.close().await;
        handler_task.abort();
        if let Some(ui) = &self.opts.ui {
            ui.done();
        }
        result
    }

    async fn solve_inner(&self, browser: &Browser, url: &str) -> Result<Clearance, FetchError> {
        let page = browser.new_page(url).await.map_err(browser_err)?;

        let user_agent: String = page
            .evaluate("navigator.userAgent")
            .await
            .ok()
            .and_then(|v| v.into_value::<String>().ok())
            .unwrap_or_default();

        let cancel = Arc::new(AtomicBool::new(false));
        let start = Instant::now();
        let mut prompted = false;
        loop {
            // 用户从 TUI 取消 → 中止解挑战、降级。
            if cancel.load(Ordering::Relaxed) {
                return Err(FetchError::Challenged(format!("用户取消解挑战 @ {url}")));
            }
            if let Ok(cookies) = page.get_cookies().await
                && let Some(cookie_header) = clearance_header(&cookies)
            {
                return Ok(Clearance {
                    cookie_header,
                    user_agent,
                });
            }

            let elapsed = start.elapsed();
            if elapsed >= self.opts.total_timeout {
                return Err(FetchError::Challenged(format!("浏览器解挑战超时 @ {url}")));
            }
            // 超宽限期仍未解开 → 可能需用户点击 Turnstile:前置窗口 + 提示(绝不模拟点击)。
            if !prompted && elapsed >= self.opts.grace && challenge_visible(&page).await {
                let _ = page.execute(BringToFrontParams::default()).await;
                if let Some(ui) = &self.opts.ui {
                    ui.prompt_click(url, cancel.clone());
                }
                prompted = true;
            }
            tokio::time::sleep(self.opts.poll_interval).await;
        }
    }

    /// 渲染后 DOM 取页(方式 A):打开 `url`,跑站点自身 JS,轮询直到 `ready_for`(CSS 选择器)
    /// 在超时内出现,返回渲染后 `outerHTML`;超时未就绪返回 `Err`(供上层降级)。
    /// `headless=true` 走无头(渲染默认无头;仅登录/解挑战需 headful)。复用登录 profile(会员态)。
    pub async fn render_dom(
        &self,
        url: &str,
        ready_for: &str,
        timeout: Duration,
        headless: bool,
    ) -> Result<String, FetchError> {
        let (mut browser, handler_task, _guard) = self.launch(headless).await?;
        let result = self
            .render_dom_inner(&browser, url, ready_for, timeout)
            .await;
        let _ = browser.close().await;
        handler_task.abort();
        result
    }

    async fn render_dom_inner(
        &self,
        browser: &Browser,
        url: &str,
        ready_for: &str,
        timeout: Duration,
    ) -> Result<String, FetchError> {
        use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;
        let page = browser.new_page(url).await.map_err(browser_err)?;
        // 事件驱动等待:注入 MutationObserver,选择器一出现即 resolve(true);JS 侧 setTimeout 仅作
        // 总超时兜底(非 Rust 侧定时器轮询)。`{:?}` 把选择器转义为安全的 JS 字符串字面量。
        let timeout_ms = timeout.as_millis().min(u32::MAX as u128) as u64;
        let wait_js = format!(
            "new Promise((resolve)=>{{const s={ready_for:?};\
             if(document.querySelector(s))return resolve(true);\
             const o=new MutationObserver(()=>{{if(document.querySelector(s)){{o.disconnect();resolve(true);}}}});\
             o.observe(document.documentElement,{{childList:true,subtree:true}});\
             setTimeout(()=>{{o.disconnect();resolve(false);}},{timeout_ms});}})"
        );
        let params = EvaluateParams::builder()
            .expression(wait_js)
            .await_promise(true)
            .return_by_value(true)
            .build()
            .map_err(|e| FetchError::Browser(format!("构建 evaluate 参数失败: {e}")))?;
        let ready = page
            .evaluate(params)
            .await
            .map_err(browser_err)?
            .into_value::<bool>()
            .unwrap_or(false);
        if !ready {
            return Err(FetchError::Challenged(format!(
                "渲染就绪超时(等待「{ready_for}」)@ {url}"
            )));
        }
        // outerHTML 取值失败也作为明确错误返回(供降级/诊断),而非静默空串。
        page.evaluate("document.documentElement.outerHTML")
            .await
            .map_err(browser_err)?
            .into_value::<String>()
            .map_err(|e| FetchError::Browser(format!("渲染后取 DOM 失败: {e} @ {url}")))
    }

    /// 启动浏览器(render/solve/login 共用):取进程内 [`BROWSER_LOCK`] 串行化(避免并发抢
    /// 同一 profile 的 `SingletonLock`)→ 清单例锁 → `headless=false` 才 `with_head()`。
    /// 返回 `(browser, handler_task, lock_guard)`:**guard 必须持到 `browser.close()` 之后**
    /// (drop 即释放串行锁);调用方用毕须 `browser.close()` + `handler_task.abort()`。
    async fn launch(
        &self,
        headless: bool,
    ) -> Result<
        (
            Browser,
            tokio::task::JoinHandle<()>,
            tokio::sync::MutexGuard<'static, ()>,
        ),
        FetchError,
    > {
        // 串行锁:持到调用方 close,保证同一时刻只有一个浏览器占用 profile。
        let guard = BROWSER_LOCK.lock().await;
        for name in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
            let _ = std::fs::remove_file(self.opts.profile_dir.join(name));
        }
        let mut builder = BrowserConfig::builder()
            .chrome_executable(&self.exe)
            .user_data_dir(&self.opts.profile_dir)
            .arg("--no-first-run")
            .arg("--no-default-browser-check");
        if !headless {
            // 解挑战(headful):chromiumoxide 默认参数含 `--enable-automation`,它让 Chrome 显示
            // 「受自动化控制」并改 window.chrome,是 Cloudflare 识别 CDP 自动化的经典信号——实测会导致
            // 用户点过 Turnstile、CF reload 后仍反复重新挑战、`cf_clearance` 永不签发(cookie 卡在 `cf_chl_*`)。
            // `disable_default_args` 拔掉含它的全部默认参数(其余多为噪声/性能项),`hide` 补回
            // `--disable-blink-features=AutomationControlled`(隐藏 navigator.webdriver 的 blink 特征)。
            builder = builder.disable_default_args().hide().with_head();
        } else {
            // 渲染(headless):保持原参数(番茄渲染流已验证可用,不动)。
            builder = builder.arg("--disable-blink-features=AutomationControlled");
        }
        let config = builder.build().map_err(FetchError::Browser)?;
        let (browser, mut handler) = Browser::launch(config).await.map_err(browser_err)?;
        let handler_task = tokio::spawn(async move { while handler.next().await.is_some() {} });
        Ok((browser, handler_task, guard))
    }

    /// 渲染 + CDP 拦截(方式 B PoC):打开 `url`,跑站点自身 JS(SPA 用 sec_sdk 自签名发请求),
    /// 通过 CDP Network 域拦截 **URL 含 `api_contains`** 的响应体并返回(签名只在浏览器内用,我们只取结果)。
    /// `headless=true` 走无头(更快、无窗口;登录之外的渲染适用,但需实测 sec_sdk 是否拒签无头)。
    /// 复用登录过的 profile,故拦截到的是会员态结果。
    pub async fn render_intercept(
        &self,
        url: &str,
        api_contains: &str,
        timeout: Duration,
        headless: bool,
    ) -> Result<String, FetchError> {
        let (mut browser, handler_task, _guard) = self.launch(headless).await?;
        let result = self
            .intercept_inner(&browser, url, api_contains, timeout)
            .await;
        let _ = browser.close().await;
        handler_task.abort();
        result
    }

    async fn intercept_inner(
        &self,
        browser: &Browser,
        url: &str,
        api_contains: &str,
        timeout: Duration,
    ) -> Result<String, FetchError> {
        use chromiumoxide::cdp::browser_protocol::network::{
            EnableParams, EventLoadingFinished, EventResponseReceived,
        };
        // 先建空白页 + 开 Network + 挂监听(responseReceived 拿 request_id;loadingFinished 是
        // body 完成的**精确事件信号**),再导航,避免错过 SPA 启动即发的请求。
        let page = browser.new_page("about:blank").await.map_err(browser_err)?;
        page.execute(EnableParams::default())
            .await
            .map_err(browser_err)?;
        let mut responses = page
            .event_listener::<EventResponseReceived>()
            .await
            .map_err(browser_err)?;
        let mut finished = page
            .event_listener::<EventLoadingFinished>()
            .await
            .map_err(browser_err)?;
        page.goto(url).await.map_err(browser_err)?;

        let deadline = Instant::now() + timeout;
        // ① 事件驱动:等到 URL 含 api_contains 的 responseReceived,拿其 request_id。
        let mut rid = None;
        while rid.is_none() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, responses.next()).await {
                Ok(Some(ev)) if ev.response.url.contains(api_contains) => {
                    rid = Some(ev.request_id.clone());
                }
                Ok(Some(_)) => {}           // 其它响应,继续
                Ok(None) | Err(_) => break, // 流结束 / 超时
            }
        }
        let Some(rid) = rid else {
            return Err(FetchError::Challenged(format!(
                "未拦截到含「{api_contains}」的响应(疑似未发出/被风控)@ {url}"
            )));
        };

        // ② 先试取一次(快响应 body 可能已就绪);否则**等该 request 的 loadingFinished**
        //    (body 完成的事件信号,不靠定时器轮询)再取,避免取到半截/空 body。
        if let Some(body) = response_body(&page, &rid).await {
            return Ok(body);
        }
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, finished.next()).await {
                Ok(Some(ev)) if ev.request_id == rid => {
                    if let Some(body) = response_body(&page, &rid).await {
                        return Ok(body);
                    }
                    break; // loadingFinished 已到但仍取不到 body → 报错
                }
                Ok(Some(_)) => {}           // 其它请求的 finished,继续
                Ok(None) | Err(_) => break, // 流结束 / 超时
            }
        }
        Err(FetchError::Challenged(format!(
            "拦截到含「{api_contains}」的响应但读取 body 失败/为空 @ {url}"
        )))
    }
}

/// 取一个请求的响应体并按 CDP `base64_encoded` 标志解码;取不到/为空/解码失败返回 `None`。
async fn response_body(
    page: &Page,
    rid: &chromiumoxide::cdp::browser_protocol::network::RequestId,
) -> Option<String> {
    use chromiumoxide::cdp::browser_protocol::network::GetResponseBodyParams;
    let resp = page
        .execute(GetResponseBodyParams::new(rid.clone()))
        .await
        .ok()?;
    if resp.result.body.is_empty() {
        return None;
    }
    if resp.result.base64_encoded {
        use base64::Engine as _;
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(&resp.result.body)
            .ok()?;
        Some(String::from_utf8_lossy(&bytes).into_owned())
    } else {
        Some(resp.result.body)
    }
}

impl BrowserFetcher {
    /// headful 打开 `url` 让用户在真实页面**手动登录**,轮询直到成功判定满足
    /// (`criteria` 的目标 cookie/localStorage 键出现,或用户在 TUI 确认 `signal.done`),
    /// 提取 cookie(含 HttpOnly,经 CDP)+ localStorage + 登录后页面 HTML 返回。
    /// 启动失败 / 用户取消(`signal.cancel`)/ 超时 → `Err`(上层降级到脚本登录或提示)。
    pub async fn login(
        &self,
        url: &str,
        criteria: &LoginCriteria,
        signal: &LoginSignal,
    ) -> Result<LoginOutcome, FetchError> {
        // 经 launch 统一启动:取 BROWSER_LOCK 串行化 + 清单例锁;登录必须 headful。
        let (mut browser, handler_task, _guard) = self.launch(false).await?;

        let result = self.login_inner(&browser, url, criteria, signal).await;

        // 生命周期:无论成败都关闭浏览器、回收事件循环(同 solve)。
        let _ = browser.close().await;
        handler_task.abort();
        result
    }

    async fn login_inner(
        &self,
        browser: &Browser,
        url: &str,
        criteria: &LoginCriteria,
        signal: &LoginSignal,
    ) -> Result<LoginOutcome, FetchError> {
        let page = browser.new_page(url).await.map_err(browser_err)?;
        // 立即前置窗口,让用户看到登录页(绝不模拟点击/填表,纯人工)。
        let _ = page.execute(BringToFrontParams::default()).await;

        let start = Instant::now();
        // 区分「取数失败(浏览器被关 / CDP 断链)」与「尚无目标值」:
        // - 连续失败计数:跨域跳转/导航瞬间 get_cookies 可能瞬时报错,单次失败不误杀;
        // - 最近一次成功读取的 cookie 快照:兼容「登录完顺手关窗、再回终端按 Enter」的自然操作流
        //   (快照最多滞后一个轮询间隔,属降级兜底)。
        let mut consecutive_failures = 0u32;
        let mut last_good: Vec<Cookie> = Vec::new();
        loop {
            if signal.cancel.load(Ordering::Relaxed) {
                return Err(FetchError::Challenged(format!("用户取消登录 @ {url}")));
            }
            let cookies = match page.get_cookies().await {
                Ok(c) => {
                    consecutive_failures = 0;
                    last_good = c.clone();
                    c
                }
                Err(_) => {
                    consecutive_failures += 1;
                    if signal.done.load(Ordering::Relaxed) {
                        // 用户已确认完成但页面取不到数(浏览器已关闭):优先用最近一次成功快照
                        // 成交(localStorage/HTML 已不可得,降级为空);快照也空则明确报错——
                        // 绝不把空登录态当「成功」落盘。
                        if !last_good.is_empty() {
                            return Ok(LoginOutcome {
                                cookies: last_good.into_iter().map(to_browser_cookie).collect(),
                                local_storage: BTreeMap::new(),
                                html: String::new(),
                                url: url.to_string(),
                            });
                        }
                        return Err(FetchError::Challenged(format!(
                            "浏览器已关闭、未能读取登录态 @ {url}(请重试,登录完成后先回终端按 Enter 再关浏览器)"
                        )));
                    }
                    // 连续多次失败 → 浏览器已被关闭 / CDP 死链:数秒内报错,而非傻轮询到登录超时。
                    if consecutive_failures >= 3 {
                        return Err(FetchError::Challenged(format!(
                            "浏览器已关闭或连接中断 @ {url}"
                        )));
                    }
                    tokio::time::sleep(self.opts.poll_interval).await;
                    continue;
                }
            };
            let local_storage = read_local_storage(&page).await;
            // 成功:用户确认,或目标 cookie/localStorage 键出现非空。
            let by_criteria = criteria
                .cookie_names
                .iter()
                .any(|n| cookies.iter().any(|c| &c.name == n && !c.value.is_empty()))
                || criteria
                    .local_storage_keys
                    .iter()
                    .any(|k| local_storage.get(k).is_some_and(|v| !v.is_empty()));
            if signal.done.load(Ordering::Relaxed) || by_criteria {
                // HTML/URL 经 evaluate 取(避免不同 chromiumoxide 版本的 page API 差异)。
                let html = page
                    .evaluate("document.documentElement.outerHTML")
                    .await
                    .ok()
                    .and_then(|v| v.into_value::<String>().ok())
                    .unwrap_or_default();
                let final_url = page
                    .evaluate("location.href")
                    .await
                    .ok()
                    .and_then(|v| v.into_value::<String>().ok())
                    .unwrap_or_else(|| url.to_string());
                let cookies = cookies.into_iter().map(to_browser_cookie).collect();
                return Ok(LoginOutcome {
                    cookies,
                    local_storage,
                    html,
                    url: final_url,
                });
            }
            if start.elapsed() >= self.opts.login_timeout {
                return Err(FetchError::Challenged(format!("浏览器登录超时 @ {url}")));
            }
            tokio::time::sleep(self.opts.poll_interval).await;
        }
    }
}

/// CDP cookie → 登录产物 cookie。
fn to_browser_cookie(c: Cookie) -> BrowserCookie {
    BrowserCookie {
        domain: c.domain,
        name: c.name,
        value: c.value,
    }
}

fn browser_err(e: chromiumoxide::error::CdpError) -> FetchError {
    FetchError::Browser(e.to_string())
}

/// 读页面 localStorage 为键值表(取 JWT 等登录态);读失败 / 无则返回空表。
async fn read_local_storage(page: &Page) -> BTreeMap<String, String> {
    const JS: &str = r#"(function(){var o={};try{for(var i=0;i<localStorage.length;i++){var k=localStorage.key(i);o[k]=localStorage.getItem(k);}}catch(e){}return JSON.stringify(o);})()"#;
    page.evaluate(JS)
        .await
        .ok()
        .and_then(|v| v.into_value::<String>().ok())
        .and_then(|s| serde_json::from_str::<BTreeMap<String, String>>(&s).ok())
        .unwrap_or_default()
}

/// 把浏览器 cookie 拼成可注入 reqwest 的 Cookie 头(仅 `cf_clearance` / `__cf*` 通行证)。
/// 无 `cf_clearance` 视为尚未解开,返回 `None`。
fn clearance_header(cookies: &[Cookie]) -> Option<String> {
    let mut parts = Vec::new();
    let mut has_clearance = false;
    for c in cookies {
        if c.name == "cf_clearance" {
            has_clearance = true;
            parts.push(format!("{}={}", c.name, c.value));
        } else if c.name.starts_with("__cf") {
            parts.push(format!("{}={}", c.name, c.value));
        }
    }
    has_clearance.then(|| parts.join("; "))
}

/// 页面是否仍停在挑战页(标题 / Turnstile iframe 判定)。
async fn challenge_visible(page: &Page) -> bool {
    const JS: &str = r#"document.title.indexOf('Just a moment')>=0
        || document.title.indexOf('请稍候')>=0
        || !!document.querySelector('iframe[src*="challenges.cloudflare.com"]')"#;
    page.evaluate(JS)
        .await
        .ok()
        .and_then(|v| v.into_value::<bool>().ok())
        .unwrap_or(false)
}
