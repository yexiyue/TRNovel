//! 基于系统已装浏览器的反爬取页(`browser` feature)。
//!
//! 复用用户系统里的 Chromium 系浏览器(Chrome/Edge/Brave/…),**headful** 解 Cloudflare
//! 托管挑战、签发 `cf_clearance`,再把 cookie + 浏览器真实 UA 交回 reqwest 取页
//! (「cookie 烤箱」,见 OpenSpec change `browser-fetcher` 的 design)。
//!
//! 注:本模块的浏览器交互**仅编译验证**,真机联调留待运行环境(CI/沙箱跑不了浏览器)。

use crate::error::FetchError;
use crate::fetch::{FetchRequest, FetchResponse, Fetcher, ReqwestFetcher};
use crate::source::BookSource;
use async_trait::async_trait;
use chromiumoxide::cdp::browser_protocol::network::Cookie;
use chromiumoxide::cdp::browser_protocol::page::BringToFrontParams;
use chromiumoxide::{Browser, BrowserConfig, Page};
use futures_util::StreamExt;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// 本会话浏览器解挑战是否已判定不可用(启动失败等)。一旦置真,后续撞挑战直接降级,
/// **不再反复启动浏览器**(避免页面反复重跑取页时浏览器频闪)。重启 app 复位。
static SOLVE_FAILED: AtomicBool = AtomicBool::new(false);

/// 渲染型取页(`render-fetcher`)本会话是否已判定不可用(启动反复失败/被拒签)。置真后
/// render 请求直接降级,避免每次搜索都重开浏览器频闪(与 [`SOLVE_FAILED`] 对称)。重启 app 复位。
static RENDER_FAILED: AtomicBool = AtomicBool::new(false);

/// 浏览器会话进程内串行锁:render/solve/login **共享同一持久 profile**(`~/.novel/browser-profile`),
/// 并发启动会抢 `SingletonLock` 互相挤崩/频闪。所有启动浏览器的路径都经 [`BrowserFetcher::spawn_browser`]
/// 启动(render 经常驻池 `new_pool_page` 懒启动/复用、headful solve/login 经 `launch_ephemeral`),三者
/// 均在此锁之下:render 持锁到 `page.close()`(Browser 常驻复用),ephemeral 持锁到 `browser.close()`。
/// 锁序固定 `BROWSER_LOCK → RENDER_POOL`,保证同一时刻只有一个浏览器实例占用 profile。
static BROWSER_LOCK: Mutex<()> = Mutex::const_new(());

/// 渲染失败时的有界重试次数(瞬态/风控常重渲即恢复;总尝试 = 1 + 重试)。
const RENDER_RETRY: u32 = 1;

/// 解挑战产出:可注入 reqwest 的 Cookie 头 + 浏览器真实 UA。
///
/// UA 必须随 cookie 一起带走:`cf_clearance` 绑签发它的 UA(见 design D6)。
#[derive(Debug, Clone)]
pub struct Clearance {
    pub cookie_header: String,
    pub user_agent: String,
}

/// 浏览器登录提取的一条 cookie(含 HttpOnly;经 CDP `get_cookies` 取得,reqwest 拿不到 HttpOnly)。
#[derive(Debug, Clone)]
pub struct BrowserCookie {
    pub domain: String,
    pub name: String,
    pub value: String,
}

/// headful 浏览器登录产出:cookie(含 HttpOnly)+ localStorage + 登录后页面(见 design D6)。
#[derive(Debug, Clone, Default)]
pub struct LoginOutcome {
    /// 浏览器全部 cookie(含 HttpOnly)。
    pub cookies: Vec<BrowserCookie>,
    /// localStorage 键值(站点把 JWT 存这里时由此取出)。
    pub local_storage: BTreeMap<String, String>,
    /// 登录后页面 HTML(可选用作 refetch / 直接解析)。
    pub html: String,
    /// 登录后页面最终 URL。
    pub url: String,
}

impl LoginOutcome {
    /// 按**注册域(eTLD+1)**归并 cookie 为 `注册域 -> "k=v; k2=v2"`,供并入 cookie 库 / 落盘
    /// (与 [`crate::fetch::cookie::CookieJar::from_persistent`] / [`crate::Engine::with_cookies`] 对接)。
    pub fn cookies_by_registrable_domain(&self) -> BTreeMap<String, String> {
        use crate::fetch::cookie::{pairs_to_str, registrable_domain};
        let mut by: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for c in &self.cookies {
            let dom = registrable_domain(c.domain.trim_start_matches('.'));
            by.entry(dom)
                .or_default()
                .insert(c.name.clone(), c.value.clone());
        }
        by.into_iter()
            .map(|(d, kv)| (d, pairs_to_str(&kv)))
            .collect()
    }
}

/// 登录成功的判定条件:任一目标 cookie 名 / localStorage 键出现非空即视为成功。
/// 二者皆空时仅靠用户在 TUI 确认([`LoginSignal::done`])。
#[derive(Debug, Clone, Default)]
pub struct LoginCriteria {
    pub cookie_names: Vec<String>,
    pub local_storage_keys: Vec<String>,
}

/// 登录交互信号:由 TUI/调用方持同一 `Arc` 翻转(用原子标志轮询,替代 Java LockSupport park/unpark)。
#[derive(Debug, Clone, Default)]
pub struct LoginSignal {
    /// 用户「登录完成」。
    pub done: Arc<AtomicBool>,
    /// 用户取消登录。
    pub cancel: Arc<AtomicBool>,
}

impl LoginSignal {
    /// 复位 `done`/`cancel` 标志。每次发起新一轮登录前须调用:信号跨登录尝试共享同一 `Arc`,
    /// 残留 `cancel` 会让重试在首轮轮询即被判「用户取消」而立即失败;残留 `done` 更危险——
    /// 会把下一次尝试的未登录/空 cookie 当「成功」落盘。
    pub fn reset(&self) {
        self.done.store(false, Ordering::Relaxed);
        self.cancel.store(false, Ordering::Relaxed);
    }
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
    /// 非阻断提醒:浏览器仍会继续工作,调用方可选择在 UI 中展示。
    fn notice(&self, _message: &str) {}
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
    /// 登录总超时(用户手动登录/2FA 较慢,故远大于解挑战的 `total_timeout`)。
    pub login_timeout: Duration,
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
            login_timeout: Duration::from_secs(300),
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

mod escalating;
mod fetcher;

pub use escalating::EscalatingFetcher;
pub use fetcher::{BrowserFetcher, shutdown_render_pool};
