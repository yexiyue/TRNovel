//! 基于系统已装浏览器的反爬取页(`browser` feature)。
//!
//! 复用用户系统里的 Chromium 系浏览器(Chrome/Edge/Brave/…),**headful** 解 Cloudflare
//! 托管挑战、签发 `cf_clearance`,再把 cookie + 浏览器真实 UA 交回 reqwest 取页
//! (「cookie 烤箱」,见 OpenSpec change `browser-fetcher` 的 design)。
//!
//! 注:本模块的浏览器交互**仅编译验证**,真机联调留待运行环境(CI/沙箱跑不了浏览器)。

use super::error::FetchError;
use super::fetch::{FetchRequest, FetchResponse, Fetcher, ReqwestFetcher};
use super::source::BookSource;
use async_trait::async_trait;
use chromiumoxide::cdp::browser_protocol::network::Cookie;
use chromiumoxide::cdp::browser_protocol::page::BringToFrontParams;
use chromiumoxide::{Browser, BrowserConfig, Page};
use futures_util::StreamExt;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// 本会话浏览器解挑战是否已判定不可用(启动失败等)。一旦置真,后续撞挑战直接降级,
/// **不再反复启动浏览器**(避免页面反复重跑取页时浏览器频闪)。重启 app 复位。
static SOLVE_FAILED: AtomicBool = AtomicBool::new(false);

/// 解挑战产出:可注入 reqwest 的 Cookie 头 + 浏览器真实 UA。
///
/// UA 必须随 cookie 一起带走:`cf_clearance` 绑签发它的 UA(见 design D6)。
#[derive(Debug, Clone)]
pub struct Clearance {
    pub cookie_header: String,
    pub user_agent: String,
}

/// 浏览器授权决定(由 [`BrowserUi::authorize`] 返回)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthDecision {
    /// 本次允许。
    Once,
    /// 总是允许(实现方应持久化)。
    Always,
    /// 拒绝:不开浏览器,降级。
    Deny,
}

/// 解挑战期间与用户交互的 UI 回调(由 app/TUI 实现;非交互场景可不提供)。
#[async_trait]
pub trait BrowserUi: Send + Sync {
    /// 撞挑战、需要打开浏览器前征求用户授权(可 await 用户决定)。
    async fn authorize(&self, source_name: &str) -> AuthDecision;
    /// 出现 Turnstile 勾选框:提示用户去弹出的浏览器里点「确认您是真人」。
    /// 用户主动取消时把 `cancel` 置真,解挑战会随即中止并降级。
    fn prompt_click(&self, url: &str, cancel: Arc<AtomicBool>);
    /// 解挑战结束(成功 / 失败 / 取消),撤下提示。
    fn done(&self);
}

/// 浏览器解挑战的可调参数。
#[derive(Clone)]
pub struct BrowserOptions {
    /// 持久化 profile 目录(养号,降低被升级为勾选框的概率,见 design D4)。
    pub profile_dir: PathBuf,
    /// 非交互宽限期:超过仍未解开则视为可能需用户点击。
    pub grace: Duration,
    /// 总超时上限(到点放弃,交上层降级,见 design D5/D11)。
    pub total_timeout: Duration,
    /// 轮询间隔。
    pub poll_interval: Duration,
    /// 交互式 UI 回调(可选;授权 + Turnstile 点击提示)。
    pub ui: Option<Arc<dyn BrowserUi>>,
}

impl Default for BrowserOptions {
    fn default() -> Self {
        Self {
            profile_dir: default_profile_dir(),
            grace: Duration::from_secs(5),
            total_timeout: Duration::from_secs(60),
            poll_interval: Duration::from_millis(800),
            ui: None,
        }
    }
}

/// 默认 profile 目录:`~/.novel/browser-profile`(与 app 的 `~/.novel` 对齐)。
fn default_profile_dir() -> PathBuf {
    match std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) {
        Some(home) => PathBuf::from(home).join(".novel").join("browser-profile"),
        None => std::env::temp_dir().join("trnovel-browser-profile"),
    }
}

/// 探测系统已装的 Chromium 系浏览器,返回可执行路径;找不到返回 `None`。
pub fn detect_browser() -> Option<PathBuf> {
    detect_browser_impl()
}

#[cfg(target_os = "macos")]
fn detect_browser_impl() -> Option<PathBuf> {
    const CANDIDATES: &[&str] = &[
        "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
        "/Applications/Microsoft Edge.app/Contents/MacOS/Microsoft Edge",
        "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser",
        "/Applications/Chromium.app/Contents/MacOS/Chromium",
        "/Applications/Vivaldi.app/Contents/MacOS/Vivaldi",
    ];
    CANDIDATES.iter().map(PathBuf::from).find(|p| p.is_file())
}

#[cfg(target_os = "windows")]
fn detect_browser_impl() -> Option<PathBuf> {
    const REL: &[&str] = &[
        r"Google\Chrome\Application\chrome.exe",
        r"Microsoft\Edge\Application\msedge.exe",
        r"BraveSoftware\Brave-Browser\Application\brave.exe",
        r"Chromium\Application\chrome.exe",
    ];
    for var in ["ProgramFiles", "ProgramFiles(x86)", "LOCALAPPDATA"] {
        let Some(root) = std::env::var_os(var).map(PathBuf::from) else {
            continue;
        };
        for rel in REL {
            let p = root.join(rel);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
fn detect_browser_impl() -> Option<PathBuf> {
    const NAMES: &[&str] = &[
        "google-chrome",
        "google-chrome-stable",
        "chromium",
        "chromium-browser",
        "microsoft-edge",
        "brave-browser",
    ];
    let paths = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&paths) {
        for name in NAMES {
            let p = dir.join(name);
            if p.is_file() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn detect_browser_impl() -> Option<PathBuf> {
    None
}

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
        // 清理上次异常退出残留的单例锁:此 profile 由本 app 独占,残留 SingletonLock
        // 会让新启动的浏览器因「profile 被占用」瞬间退出(表现为频闪 + "Browser process exit")。
        for name in ["SingletonLock", "SingletonSocket", "SingletonCookie"] {
            let _ = std::fs::remove_file(self.opts.profile_dir.join(name));
        }

        let config = BrowserConfig::builder()
            .chrome_executable(&self.exe)
            .user_data_dir(&self.opts.profile_dir)
            .with_head() // 必须 headful(headless 解不开,见 design D4)
            .arg("--no-first-run")
            .arg("--no-default-browser-check")
            .arg("--disable-blink-features=AutomationControlled")
            .build()
            .map_err(FetchError::Browser)?;

        let (mut browser, mut handler) = Browser::launch(config).await.map_err(browser_err)?;
        // 持续驱动 CDP 连接直到关闭(stream 返回 None)。
        // 不能因单个错误事件就退出 —— 否则会把正在进行的命令的响应通道丢掉,报 "oneshot canceled"。
        let handler_task = tokio::spawn(async move { while handler.next().await.is_some() {} });

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
}

fn browser_err(e: chromiumoxide::error::CdpError) -> FetchError {
    FetchError::Browser(e.to_string())
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
    use super::{EscalatingFetcher, detect_browser};
    use crate::fetch::{FetchRequest, Fetcher};
    use crate::source::BookSource;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn detect_browser_does_not_panic() {
        // 探测不应 panic;有无浏览器取决于运行机器,这里只验证可调用。
        let _ = detect_browser();
    }

    fn book_source(base: &str) -> BookSource {
        serde_json::from_value(serde_json::json!({
            "schema": "trnovel-booksource/v2",
            "name": "t",
            "url": base,
            "bookInfo": {},
            "toc": {"list": {"via": "raw"}, "name": {"via": "raw"}, "url": {"via": "raw"}},
            "content": {"value": {"via": "raw"}}
        }))
        .unwrap()
    }

    // ── 审查/correctness:EscalatingFetcher 必须覆盖 fetch_full,透传真实状态码与响应头 ──
    // 否则落到默认实现 → net.connect 静默退化为 {code:200, headers:{}},打掉登录脚本读 Set-Cookie 的能力。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn escalating_fetcher_fetch_full_passes_status_and_headers() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 2048];
                let _ = stream.read(&mut buf);
                let resp = "HTTP/1.1 201 Created\r\nSet-Cookie: sid=zzz; Path=/\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";
                let _ = stream.write_all(resp.as_bytes());
                let _ = stream.flush();
            }
        });
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
}
