//! 浏览器取页器 [`BrowserFetcher`]:headful 解 Cloudflare 挑战、渲染取页(DOM 轮询 / CDP 拦截)、手动登录。

use super::*;

/// 常驻渲染浏览器实例(`browser-pool`):render 路径复用同一 `Browser`、只开新 `Page`,
/// 避免每次 launch/close 整个浏览器的秒级开销(交互翻页顺滑;也是 P2 点击翻页的基础设施)。
///
/// 仅 render(headless)走池;`solve`/`login`(headful)仍每次临时 launch+close。
struct Resident {
    browser: Browser,
    /// 驱动 CDP 事件循环的常驻任务。**仅在显式 `Browser::close()` 后才结束**——常驻 render 浏览器
    /// 从不 close,崩溃/断连时本 task 多半 parked 在 `Pending`、`is_finished()` 仍为 false。
    /// 故 `is_finished()` 只是乐观快路检查;真正的断连兜底是 [`BrowserFetcher::new_pool_page`]
    /// 中 `new_page` 失败/超时后的拆除重建。
    handler: tokio::task::JoinHandle<()>,
    /// 本实例启动时的 headless 标志;请求标志不一致时拆掉重建。
    headless: bool,
}

/// **进程级**常驻渲染浏览器池(与 [`BROWSER_LOCK`] 同为全局 static)。
///
/// 本仓所有书源共享同一持久 profile(`~/.novel/browser-profile`),常驻浏览器**必须全局唯一**:
/// 若每个 [`BrowserFetcher`] 各持一个,两个实例(如多书源/路由回退栈)各自的常驻浏览器会同时存活、
/// 互抢 profile 的 `SingletonLock`(后建者 [`BrowserFetcher::spawn_browser`] 会删掉前者仍占用的锁)→
/// profile 数据竞争/「配置文件已在使用」。提为全局单例后,`SingletonLock` 永远单一所有者,且
/// [`shutdown_render_pool`] 能腾掉任何路径留下的常驻浏览器(满足 design D1「单个常驻 Browser」)。
///
/// 访问恒在 [`BROWSER_LOCK`] 之下(锁序 `BROWSER_LOCK → RENDER_POOL`);用 tokio `Mutex` 以便把池内
/// `Browser` 引用跨 await 持有(渲染期间)。app 退出由 [`shutdown_render_pool`] 收尾(static 不触发 `Drop`)。
static RENDER_POOL: Mutex<Option<Resident>> = Mutex::const_new(None);

/// chromiumoxide 0.9 `DEFAULT_ARGS` **去掉 `--enable-automation` 后**的等价集。
///
/// headful 解挑战 / 登录时,为去掉 CF 据以死循环的 `--enable-automation` 必须 `disable_default_args()`,
/// 但该开关「全有或全无」——会连带拔掉抑制**浏览器首次运行体验(FRE)**的 `--disable-sync` /
/// `--disable-default-apps` / `--disable-client-side-phishing-detection` 等;Edge 尤其会因此弹
/// 「欢迎使用 Microsoft Edge / 同步登录」模态挡住挑战页,用户无从点击,解挑战卡死(headless 渲染路径
/// 保留默认参数故无此问题)。这里手动补回这些项(独缺 `--enable-automation`)。
///
/// 略去对解挑战无益的 `--lang=en_US`(保用户原生 UI 语言)与 `--enable-blink-features=IdleDetection`;
/// `--no-first-run` 由 [`BrowserFetcher::spawn_browser`] 统一加。**随 chromiumoxide 版本(钉 0.9.1)更新需复核**。
const HEADFUL_DEFAULT_ARGS: &[&str] = &[
    "--disable-background-networking",
    "--enable-features=NetworkService,NetworkServiceInProcess",
    "--disable-background-timer-throttling",
    "--disable-backgrounding-occluded-windows",
    "--disable-breakpad",
    "--disable-client-side-phishing-detection",
    "--disable-component-extensions-with-background-pages",
    "--disable-default-apps",
    "--disable-dev-shm-usage",
    "--disable-features=TranslateUI",
    "--disable-hang-monitor",
    "--disable-ipc-flooding-protection",
    "--disable-popup-blocking",
    "--disable-prompt-on-repost",
    "--disable-renderer-backgrounding",
    "--disable-sync",
    "--force-color-profile=srgb",
    "--metrics-recording-only",
    "--password-store=basic",
    "--use-mock-keychain",
];

/// 基于系统浏览器的解挑战器(cookie 烤箱)。常驻渲染浏览器是进程级共享的 `RENDER_POOL`,
/// 不随本结构存活(故无 `Drop` 收尾——见 [`shutdown_render_pool`])。
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
        // 临时浏览器(headful;headless 解不开):经 with_ephemeral 统一收尾(close 先于 abort)。
        let result = self
            .with_ephemeral(false, async |browser| self.solve_inner(browser, url).await)
            .await;
        // 生命周期:无论成败都撤下交互提示(D11)。
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
        // 渲染走进程级常驻池:整段持 BROWSER_LOCK,从池开新 Page 导航 url、用完关 Page 留 Browser。
        self.with_pool_page(url, headless, async |page| {
            Self::render_dom_page(page, url, ready_for, timeout).await
        })
        .await
    }

    /// 渲染后 DOM 取页核心:等 `ready_for`(CSS 选择器)在 `timeout` 内出现,返回渲染后 `outerHTML`;
    /// 超时未就绪返回 `Err`(供上层降级)。
    async fn render_dom_page(
        page: &Page,
        url: &str,
        ready_for: &str,
        timeout: Duration,
    ) -> Result<String, FetchError> {
        if !Self::wait_ready(page, ready_for, timeout).await? {
            return Err(FetchError::Challenged(format!(
                "渲染就绪超时(等待「{ready_for}」)@ {url}"
            )));
        }
        Self::outer_html(page).await
    }

    /// 事件驱动等待 `ready_for`(CSS 选择器)在 `timeout` 内出现:注入 MutationObserver,一出现即
    /// resolve(true);JS 侧 setTimeout 作总超时兜底(非 Rust 侧定时器轮询)。返回是否就绪。
    /// `{:?}` 把选择器转义为安全的 JS 字符串字面量。
    async fn wait_ready(
        page: &Page,
        ready_for: &str,
        timeout: Duration,
    ) -> Result<bool, FetchError> {
        use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;
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
        Ok(page
            .evaluate(params)
            .await
            .map_err(browser_err)?
            .into_value::<bool>()
            .unwrap_or(false))
    }

    /// 取渲染后整页 `outerHTML`;取值失败作明确错误(供降级/诊断),而非静默空串。
    async fn outer_html(page: &Page) -> Result<String, FetchError> {
        page.evaluate("document.documentElement.outerHTML")
            .await
            .map_err(browser_err)?
            .into_value::<String>()
            .map_err(|e| FetchError::Browser(format!("渲染后取 DOM 失败: {e}")))
    }

    /// 实际启动一个浏览器进程(**不取锁**;调用方须已持 [`BROWSER_LOCK`] 并确保 profile 无其它
    /// 实例占用)。清单例锁 → 按 `headless` 配参 → `Browser::launch` + spawn handler 事件循环。
    async fn spawn_browser(
        &self,
        headless: bool,
    ) -> Result<(Browser, tokio::task::JoinHandle<()>), FetchError> {
        for name in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
            let _ = std::fs::remove_file(self.opts.profile_dir.join(name));
        }
        let mut builder = BrowserConfig::builder()
            .chrome_executable(&self.exe)
            .user_data_dir(&self.opts.profile_dir)
            .arg("--no-first-run")
            .arg("--no-default-browser-check");
        if !headless {
            // 解挑战(headful):chromiumoxide 默认参数含 `--enable-automation`,它让浏览器显示
            // 「受自动化控制」并改 window.chrome,是 Cloudflare 识别 CDP 自动化的经典信号——实测会导致
            // 用户点过 Turnstile、CF reload 后仍反复重新挑战、`cf_clearance` 永不签发(cookie 卡在 `cf_chl_*`)。
            // `disable_default_args` 是「全有或全无」:为去掉 `--enable-automation` 会把默认参数**全部**拔掉,
            // 其中 `--disable-sync`/`--disable-default-apps` 等是抑制浏览器**首次运行体验(FRE)**的关键——
            // 尤其 Edge 缺了会弹「欢迎/同步登录」模态挡住挑战页,用户无从点击「确认真人」,解挑战因此卡死。
            // 故拔掉默认参数后,手动补回「默认参数去掉 enable-automation」的等价集([`HEADFUL_DEFAULT_ARGS`]);
            // `hide` 再补 `--disable-blink-features=AutomationControlled`(隐藏 navigator.webdriver 的 blink 特征)。
            builder = builder.disable_default_args().hide().with_head();
            for &arg in HEADFUL_DEFAULT_ARGS {
                builder = builder.arg(arg);
            }
        } else {
            // 渲染(headless):保持 chromiumoxide 默认参数(含 `--disable-sync` 等,无 FRE 弹窗问题;
            // 番茄渲染流已验证可用,不动)。
            builder = builder.arg("--disable-blink-features=AutomationControlled");
        }
        let config = builder.build().map_err(FetchError::Browser)?;
        let (browser, mut handler) = Browser::launch(config).await.map_err(browser_err)?;
        let handler_task = tokio::spawn(async move { while handler.next().await.is_some() {} });
        Ok((browser, handler_task))
    }

    /// 临时浏览器启动(headful 解挑战 / 登录用):取 [`BROWSER_LOCK`] 串行化 → **先拆掉常驻渲染
    /// 浏览器**([`shutdown_render_pool`],否则两个实例抢同一 profile 的 `SingletonLock`)→ 起一个
    /// 一次性实例。返回 `(browser, handler_task, lock_guard)`:**guard 必须持到 `browser.close()`
    /// 之后**。一般经 [`BrowserFetcher::with_ephemeral`] 调用以统一收尾。
    async fn launch_ephemeral(
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
        let guard = BROWSER_LOCK.lock().await;
        // 常驻渲染浏览器还活着会占住 profile:先优雅关掉,腾出 SingletonLock 给本次 headful。
        shutdown_render_pool().await;
        let (browser, handler_task) = self.spawn_browser(headless).await?;
        Ok((browser, handler_task, guard))
    }

    /// 临时浏览器生命周期收口(`solve`/`login` 共用):取临时实例 → 跑 `f(&browser)` → 收尾。
    /// **`close().await` 固定先于 `handler.abort()`**——`Browser::close` 要经 handler 转发关闭命令,
    /// 先 abort 会让 close 拿不到应答。收尾顺序只此一处定义。
    async fn with_ephemeral<T>(
        &self,
        headless: bool,
        f: impl AsyncFnOnce(&Browser) -> Result<T, FetchError>,
    ) -> Result<T, FetchError> {
        let (mut browser, handler_task, _guard) = self.launch_ephemeral(headless).await?;
        let result = f(&browser).await;
        let _ = browser.close().await;
        handler_task.abort();
        result
    }

    /// 渲染取页生命周期收口(`browser-pool`,`render_dom`/`render_intercept` 共用):整段持
    /// [`BROWSER_LOCK`] → 从 [`RENDER_POOL`] 开新 `Page`(导航 `open_url`)→ 跑 `f(&page)` →
    /// 关 `Page` 留 `Browser`(下次翻页复用、免每次 launch)。池 guard 持到渲染结束(无并发渲染,
    /// 见 design Non-Goals);`BROWSER_LOCK` 与 solve/login 互斥同一 profile。
    async fn with_pool_page<T>(
        &self,
        open_url: &str,
        headless: bool,
        f: impl AsyncFnOnce(&Page) -> Result<T, FetchError>,
    ) -> Result<T, FetchError> {
        let _guard = BROWSER_LOCK.lock().await;
        let mut pool = RENDER_POOL.lock().await;
        let page = self.new_pool_page(&mut pool, headless, open_url).await?;
        let result = f(&page).await;
        let _ = page.close().await; // 关 Page、留 Browser(池复用关键)
        result
    }

    /// 从常驻池取一个新 `Page`(`browser-pool` 核心):懒启动 / 用前快路检查(`handler` 已结束 /
    /// headless 不符)/ 开页失败或超时(视为断连)→ 拆掉重建,至多重建一次。
    /// 调用方须已持 [`BROWSER_LOCK`] 并传入 [`RENDER_POOL`] 槽的可变借用。
    async fn new_pool_page(
        &self,
        pool: &mut Option<Resident>,
        headless: bool,
        url: &str,
    ) -> Result<Page, FetchError> {
        // 死浏览器的 `new_page` 不会立刻报错——handler task 多半 parked,要等 CDP 默认 30s 请求超时
        // 才回错,期间还独占 [`BROWSER_LOCK`] 阻塞所有书源。故给开页套一道短超时,超时即判断连、拆掉重建。
        // (`spawn_browser` 的进程冷启动在本超时之外,不会被误判。)
        const OPEN_PAGE_TIMEOUT: Duration = Duration::from_secs(8);
        let mut last_err = None;
        for _ in 0..2 {
            // 快路检查:存在 + handler 未结束 + headless 一致;否则拆旧建新(懒启动/重建)。
            let healthy = matches!(
                pool.as_ref(),
                Some(r) if !r.handler.is_finished() && r.headless == headless
            );
            if !healthy {
                drop_resident(pool); // 同步拆(可能已死,不能 await close)
                let (browser, handler) = self.spawn_browser(headless).await?;
                *pool = Some(Resident {
                    browser,
                    handler,
                    headless,
                });
            }
            // 开页(带超时):成功即返回;失败/超时说明实例已坏 → 拆掉,循环重建一次。
            let opened = {
                let browser = &pool.as_ref().expect("健康检查后池必为 Some").browser;
                tokio::time::timeout(OPEN_PAGE_TIMEOUT, browser.new_page(url)).await
            };
            match opened {
                Ok(Ok(page)) => return Ok(page),
                Ok(Err(e)) => {
                    drop_resident(pool);
                    last_err = Some(browser_err(e));
                }
                Err(_) => {
                    drop_resident(pool);
                    last_err = Some(FetchError::Browser(format!(
                        "常驻浏览器开页超时(疑似断连,{OPEN_PAGE_TIMEOUT:?})"
                    )));
                }
            }
        }
        Err(last_err.unwrap_or_else(|| FetchError::Browser("常驻浏览器开页反复失败".into())))
    }

    /// 渲染 + CDP 拦截取页(`render-fetcher` / `render-dual-source` / `search-click-pagination` 统一入口):
    /// 从常驻池开一张空白 `Page`(用完关 Page 留 Browser)渲染 `url`、跑站点自身 JS(SPA 自签名发请求),
    /// 拦 **URL 含 `api_contains`** 的响应体返回(签名只在浏览器内用,我们只取结果)。
    /// - `dom_ready` 有值(`render-dual-source`):拦完 API 再等就绪闸、另抓渲染 DOM 一并返回(供 via:css 规则)。
    /// - `paging` 有值 `(target_page, next_selector)`(`search-click-pagination`):URL 不认页码的 SPA
    ///   (如番茄 search)从首页点 `next_selector` 翻到第 `target_page` 页;`None` = 单页(现状)。
    ///
    /// `headless=true` 走无头(更快、无窗口);复用登录过的 profile,故拦到的是会员态结果。
    pub async fn render_intercept(
        &self,
        url: &str,
        api_contains: &str,
        timeout: Duration,
        headless: bool,
        dom_ready: Option<&str>,
        paging: Option<(u32, &str)>,
    ) -> Result<(String, Option<String>), FetchError> {
        self.with_pool_page("about:blank", headless, async |page| {
            Self::intercept(page, url, api_contains, timeout, dom_ready, paging).await
        })
        .await
    }

    /// 拦截取页核心(单页 / 点击翻页统一):池开好的空白 `page` 上开 Network + 挂监听(`responseReceived`
    /// 拿 request_id;`loadingFinished` 是 body 完成的精确事件信号),**先挂再 `goto`**——避免错过 SPA 启动即
    /// 发的请求;两个事件流**跨整个点击循环持有**,自然只认「本次 goto/点击之后到达」的响应。流程:
    /// goto → 拦首页 API(软封锁空 body → reload-once,D5)→ `paging` 有值则点 `next_selector` 翻到
    /// `target_page`(每页按 `page_index` 对齐,排除 reload 残留 0)→ `dom_ready` 有值则抓落定后 DOM。
    /// 返回 (第 `target_page` 页 body, 可选渲染 DOM)。
    async fn intercept(
        page: &Page,
        url: &str,
        api_contains: &str,
        timeout: Duration,
        dom_ready: Option<&str>,
        paging: Option<(u32, &str)>,
    ) -> Result<(String, Option<String>), FetchError> {
        use chromiumoxide::cdp::browser_protocol::network::{
            EnableParams, EventLoadingFinished, EventResponseReceived,
        };
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

        // 首页:点击翻页按 page_index=0 对齐;单页不约束(None)。软封锁(空 body)→ reload 一次重试。
        let first_idx = paging.map(|_| 0);
        page.goto(url).await.map_err(browser_err)?;
        let mut body = Self::wait_matching_body(
            page,
            &mut responses,
            &mut finished,
            api_contains,
            timeout,
            first_idx,
        )
        .await?;
        if body.trim().is_empty() {
            page.goto(url).await.map_err(browser_err)?;
            body = Self::wait_matching_body(
                page,
                &mut responses,
                &mut finished,
                api_contains,
                timeout,
                first_idx,
            )
            .await?;
        }

        // 点击翻页:逐页点「下一页」翻到 target_page,每页按 page_index = human_page-1 对齐拦响应。
        // 控件禁用/缺失(AtEnd)= 真到头(page 超过总页数)→ 收尾返回当前(末)页;点击未令页前进
        //(拥塞/未生效)→ 重试一次(spec SHALL),重试耗尽仍未达目标页 → **报错**(交上层整页重试/降级),
        // **不静默把更早的页当成第 N 页返回**(重试若致过度翻页则 page_index 对不上 → 同样 Err)。
        if let Some((target_page, next_selector)) = paging {
            const CLICK_RETRY: u32 = 2; // 总尝试次数 = 1 次 + 1 重试
            'pages: for human_page in 2..=target_page {
                for attempt in 0..CLICK_RETRY {
                    match Self::click_next(page, next_selector).await? {
                        ClickOutcome::AtEnd => break 'pages,
                        ClickOutcome::Clicked => {}
                    }
                    match Self::wait_matching_body(
                        page,
                        &mut responses,
                        &mut finished,
                        api_contains,
                        timeout,
                        Some(human_page - 1),
                    )
                    .await
                    {
                        Ok(b) => {
                            body = b;
                            continue 'pages; // 该页到手,翻下一页
                        }
                        Err(e) if attempt + 1 >= CLICK_RETRY => return Err(e),
                        Err(_) => {} // 重试:重新滚动入视 + 重派点击
                    }
                }
            }
        }

        // 渲染 DOM(`render-dual-source`):抓 outerHTML 前对就绪闸重等一次。注:番茄场景 dom_ready
        // (`.byte-pagination`)从第 1 页起就在,故此 wait_ready 即时返回(非「等到第 N 页」屏障)——
        // 对番茄安全,因 totalPages 取末数字项(总页数各页恒定、与 active 页无关)。若未来书源的 DOM
        // 规则依赖 active 页,需改为 poll `.byte-pagination-item-active`==目标页。
        let dom = match dom_ready {
            Some(sel) => {
                let _ = Self::wait_ready(page, sel, timeout).await;
                Self::outer_html(page).await.ok()
            }
            None => None,
        };
        Ok((body, dom))
    }

    /// 在已布设监听的页上等「下一个匹配响应」的 body([`BrowserFetcher::intercept`] 首页取页与逐页
    /// 点击共用)。匹配 = URL 含 `api_contains`;`expect_idx` 有值时还要求 URL 的 `page_index`
    /// 等于它(点击翻页**按目标页对齐**,排除 reload 残留的 `page_index=0`;URL 无该参数则不约束)。
    /// 监听流跨调用持有 → 自然只认「本次调用(点击/导航)之后到达」的响应。先试取 body,否则等该
    /// request 的 `loadingFinished`(body 完成的精确信号)再取,避免取到半截/空 body。
    /// **返回约定**:匹配到响应且 body 非空 → `Ok(body)`;匹配到但 body 空(软封锁,HTTP 200 空 body)
    /// → `Ok("")`(交调用方 reload-once 兜底,D5);**完全没匹配到**响应 → `Err`(真没发出 / 被风控)。
    async fn wait_matching_body<R, F>(
        page: &Page,
        responses: &mut R,
        finished: &mut F,
        api_contains: &str,
        timeout: Duration,
        expect_idx: Option<u32>,
    ) -> Result<String, FetchError>
    where
        R: futures_util::Stream<
                Item = Arc<chromiumoxide::cdp::browser_protocol::network::EventResponseReceived>,
            > + Unpin,
        F: futures_util::Stream<
                Item = Arc<chromiumoxide::cdp::browser_protocol::network::EventLoadingFinished>,
            > + Unpin,
    {
        let deadline = Instant::now() + timeout;
        // ① 等匹配的 responseReceived,拿 request_id(api 子串 + 可选 page_index 对齐)。
        let mut rid = None;
        while rid.is_none() {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, responses.next()).await {
                Ok(Some(ev))
                    if ev.response.url.contains(api_contains)
                        && expect_idx.is_none_or(|exp| {
                            url_page_index(&ev.response.url).is_none_or(|got| got == exp)
                        }) =>
                {
                    rid = Some(ev.request_id.clone());
                }
                Ok(Some(_)) => {}           // 其它响应,继续
                Ok(None) | Err(_) => break, // 流结束 / 超时
            }
        }
        let Some(rid) = rid else {
            return Err(FetchError::Challenged(format!(
                "未拦截到含「{api_contains}」{}的响应(疑似未发出/被风控)",
                expect_idx
                    .map(|i| format!("(page_index={i})"))
                    .unwrap_or_default()
            )));
        };
        // ② 先试取一次,否则等该 request 的 loadingFinished(body 完成事件)再取。
        if let Some(body) = response_body(page, &rid).await {
            return Ok(body);
        }
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            match tokio::time::timeout(remaining, finished.next()).await {
                Ok(Some(ev)) if ev.request_id == rid => {
                    if let Some(body) = response_body(page, &rid).await {
                        return Ok(body);
                    }
                    break; // loadingFinished 已到但仍取不到 body → 报错
                }
                Ok(Some(_)) => {}           // 其它请求的 finished,继续
                Ok(None) | Err(_) => break, // 流结束 / 超时
            }
        }
        // 匹配到了响应(rid 有值),但 loadingFinished 后 body 仍为空 → 返回**空串(非 Err)**:
        // 这正是软封锁的精确信号(HTTP 200 空 body),交调用方 `if body.is_empty()` 触发 reload-once
        // 兜底(D5)。区别于上面「没匹配到任何响应」的 Err(那才是真没发出 / 被风控)。response_body
        // 对空 body 返回 None,若这里也返回 Err 会让 `.await?` 短路、reload 守卫成死代码。
        Ok(String::new())
    }

    /// 在已加载页上点「下一页」(`next_selector`):滚动入视 + 派发真实 `MouseEvent`(实测离屏
    /// CLI 单击无效、需真事件才触发站点 React 翻页);控件缺失 / 带 `disabled`(或 `aria-disabled`)
    /// → 到头(D6 结构快路,兼与「点击失败/拥塞」消歧)。返回是否实际点了。`{:?}` 把选择器转义为
    /// 安全 JS 字符串字面量。
    async fn click_next(page: &Page, next_selector: &str) -> Result<ClickOutcome, FetchError> {
        use chromiumoxide::cdp::js_protocol::runtime::EvaluateParams;
        let js = format!(
            "(function(){{const el=document.querySelector({next_selector:?});\
             if(!el)return 'missing';\
             if(el.classList.contains('disabled')||el.getAttribute('aria-disabled')==='true')return 'disabled';\
             el.scrollIntoView({{block:'center',inline:'center'}});\
             ['mousedown','mouseup','click'].forEach(t=>el.dispatchEvent(\
               new MouseEvent(t,{{bubbles:true,cancelable:true,view:window}})));\
             return 'clicked';}})()"
        );
        let params = EvaluateParams::builder()
            .expression(js)
            .return_by_value(true)
            .build()
            .map_err(|e| FetchError::Browser(format!("构建 evaluate 参数失败: {e}")))?;
        let state = page
            .evaluate(params)
            .await
            .map_err(browser_err)?
            .into_value::<String>()
            .unwrap_or_default();
        Ok(if state == "clicked" {
            ClickOutcome::Clicked
        } else {
            ClickOutcome::AtEnd // missing / disabled / 取值失败 → 到头
        })
    }
}

/// 点「下一页」的结果:实际点了(等待翻页)/ 到头(控件缺失或禁用,停止翻页)。
enum ClickOutcome {
    Clicked,
    AtEnd,
}

/// 从响应 URL 解析 `page_index=N` 的 N(点击翻页**按目标页对齐**、排除 reload 残留 `page_index=0`);
/// 无该参数 / 值非数字 → `None`(调用方据「点击后下一个匹配响应」兜底,保通用)。
/// 按 `&` 切 query 后**精确匹配** key `page_index`,避免 `item_page_index=9` 等诱饵子串误命中。
fn url_page_index(url: &str) -> Option<u32> {
    let query = url.split_once('?')?.1;
    let value = query
        .split('&')
        .find_map(|kv| kv.strip_prefix("page_index="))?;
    value
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
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
        // 临时浏览器(headful):经 with_ephemeral 统一收尾(先拆常驻渲染浏览器腾 profile,close 先于 abort)。
        self.with_ephemeral(false, async |browser| {
            self.login_inner(browser, url, criteria, signal).await
        })
        .await
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

/// 同步拆除一个常驻实例(abort handler + drop `Browser`)。用于断连重建——此时浏览器可能已死,
/// 不能 `await close()`(会挂);`Browser` 的子进程设了 `kill_on_drop`,drop 即在后台被杀/reap,
/// 不残留僵尸进程。优雅关闭(release SingletonLock)走 [`shutdown_render_pool`]。
fn drop_resident(slot: &mut Option<Resident>) {
    if let Some(r) = slot.take() {
        r.handler.abort();
        // drop(r.browser):child kill_on_drop。
    }
}

/// 关闭进程级常驻渲染浏览器(`RENDER_POOL`)。两处调用:
/// - **app 退出**(`pub`):static 不触发 `Drop`,须显式关闭,否则 headless 浏览器子进程会被孤儿化;
/// - **headful solve/login 启动前**(`launch_ephemeral`):腾出 profile 的 `SingletonLock`。
///
/// 先 `close().await` 优雅释放锁(同步 drop 不保证),3s 超时兜底(随后 drop 触发 `kill_on_drop`);
/// 再走与断连重建同一段同步收尾(`drop_resident`)。池为空时无操作。
pub async fn shutdown_render_pool() {
    let mut pool = RENDER_POOL.lock().await;
    if let Some(r) = pool.as_mut() {
        let _ = tokio::time::timeout(Duration::from_secs(3), r.browser.close()).await;
    }
    drop_resident(&mut pool);
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

#[cfg(test)]
mod tests {
    use super::url_page_index;

    // page_index 解析:按 `&` 切 query 精确匹配 key,诱饵子串不误命中(点击翻页相关性的基石)。
    #[test]
    fn url_page_index_parses_exact_key() {
        // 番茄实际形态:page_index 在中段。
        assert_eq!(
            url_page_index(
                "https://x/api/author/search/search_book/v1?filter=127&page_count=10&page_index=2&query_word=k"
            ),
            Some(2)
        );
        // 首参。
        assert_eq!(url_page_index("https://x/api?page_index=0&a=1"), Some(0));
        // 诱饵 key(以 page_index 结尾的别的参数)在前 → 不误命中,取真 key。
        assert_eq!(
            url_page_index("https://x/api?item_page_index=9&page_index=3"),
            Some(3)
        );
        // 无 page_index 参数 → None(调用方兜底为「点击后下一个匹配响应」)。
        assert_eq!(url_page_index("https://x/api?page=2&offset=10"), None);
        // 无 query。
        assert_eq!(url_page_index("https://x/api/search_book/v1"), None);
        // 空值 → None。
        assert_eq!(url_page_index("https://x/api?page_index=&foo=1"), None);
    }
}
