//! 浏览器辅助取页的 app 侧装配(反爬)。
//!
//! 撞 Cloudflare 挑战时,经用户授权后用系统浏览器解出 `cf_clearance` 再交回 reqwest
//! (见 OpenSpec change `browser-fetcher` 的 design D8/D12)。
//!
//! 授权两级:
//! - **设置开关**:`~/.novel/browser_assist.on` 标记(「总是允许」),由书源管理页 B 键或弹窗「总是」写入;
//! - **首次弹窗**:未设标记时,撞挑战会弹模态问「本次 / 总是 / 拒绝」。
//!
//! 关键:授权决定存于**模块级会话缓存**(非随某次取页 future 存活),`authorize` 以轮询等待。
//! 这样并发取页、或随 query 变化被取消重启的取页,都不会叠加/抖动弹窗——只在「当前无弹窗」时
//! 弹一次,用户决定后所有等待者复用同一决定。

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use parse_book_source::{
    AuthDecision, BookSource, BrowserFetcher, BrowserOptions, BrowserUi, Engine, FetchMode,
};
use ratatui_kit::prelude::State;
use tokio::time::sleep;

/// 待用户处理的浏览器交互(由全局上下文 `State<Option<BrowserPrompt>>` 承载,模态组件消费)。
#[derive(Clone)]
pub enum BrowserPrompt {
    /// 撞挑战、需授权:用户在模态里选「本次 / 总是 / 拒绝」(经 [`record_decision`] / [`set_always_allowed`] 落地)。
    Authorize { source_name: String },
    /// 出现 Turnstile 勾选框:提示用户去浏览器点;`cancel` 置真表示取消。
    Click {
        #[allow(dead_code)]
        url: String,
        cancel: Arc<AtomicBool>,
    },
    /// 非阻断提醒:浏览器仍会继续工作,用户按键关闭提示即可。
    Notice { message: String },
}

// ───────────────────────── 会话授权决定(本次 / 拒绝)─────────────────────────

/// 本会话的「本次允许 / 拒绝」决定缓存(「总是允许」走 [`always_allowed`] 文件,见下)。
static DECISION: Mutex<Option<AuthDecision>> = Mutex::new(None);

/// 模态在用户选「本次允许 / 拒绝」后调用,记录本会话决定(「总是」不走这里,见 [`set_always_allowed`])。
pub fn record_decision(decision: AuthDecision) {
    if let Ok(mut d) = DECISION.lock() {
        *d = Some(decision);
    }
}

fn cached_decision() -> Option<AuthDecision> {
    DECISION.lock().ok().and_then(|d| *d)
}

// ───────────────────────── 持久化:「总是允许」标记 ─────────────────────────

fn flag_path() -> Option<std::path::PathBuf> {
    crate::utils::novel_catch_dir()
        .ok()
        .map(|d| d.join("browser_assist.on"))
}

/// 是否已「总是允许」浏览器辅助验证。
pub fn always_allowed() -> bool {
    flag_path().map(|p| p.exists()).unwrap_or(false)
}

/// 设置 / 取消「总是允许」(供书源管理页 B 键开关与弹窗「总是」使用)。
pub fn set_always_allowed(on: bool) -> std::io::Result<()> {
    let Some(p) = flag_path() else {
        return Ok(());
    };
    if on {
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(&p, b"on")?;
    } else if p.exists() {
        std::fs::remove_file(&p)?;
    }
    Ok(())
}

// ───────────────────────── 全局 UI 句柄(避免到处穿参)─────────────────────────

static BROWSER_UI: OnceLock<Arc<dyn BrowserUi>> = OnceLock::new();

/// App 启动时调用一次:把全局提示状态登记为浏览器 UI 回调,供 [`build_engine`] 取用。
pub fn init_browser_ui(state: State<Option<BrowserPrompt>>) {
    let _ = BROWSER_UI.set(Arc::new(TuiBrowserUi { state }));
}

fn browser_ui() -> Option<Arc<dyn BrowserUi>> {
    BROWSER_UI.get().cloned()
}

/// 基于全局提示状态的 [`BrowserUi`] 实现:把交互投递到 TUI 模态,以轮询等待用户决定。
struct TuiBrowserUi {
    state: State<Option<BrowserPrompt>>,
}

#[async_trait]
impl BrowserUi for TuiBrowserUi {
    async fn authorize(&self, source_name: &str) -> AuthDecision {
        loop {
            // 「总是允许」(设置开关 / 弹窗选总是)→ 直接放行。
            if always_allowed() {
                return AuthDecision::Always;
            }
            // 本会话已选过「本次 / 拒绝」→ 复用,不再弹窗。
            if let Some(d) = cached_decision() {
                return d;
            }
            // 仅当当前无弹窗时弹一次(并发/取消重启都不会叠加抖动)。
            {
                let mut st = self.state.write();
                if st.is_none() {
                    *st = Some(BrowserPrompt::Authorize {
                        source_name: source_name.to_string(),
                    });
                }
            }
            sleep(Duration::from_millis(150)).await;
        }
    }

    fn prompt_click(&self, url: &str, cancel: Arc<AtomicBool>) {
        *self.state.write() = Some(BrowserPrompt::Click {
            url: url.to_string(),
            cancel,
        });
    }

    fn notice(&self, message: &str) {
        *self.state.write() = Some(BrowserPrompt::Notice {
            message: message.to_string(),
        });
    }

    fn done(&self) {
        if !matches!(
            self.state.read().as_ref(),
            Some(BrowserPrompt::Notice { .. })
        ) {
            *self.state.write() = None;
        }
    }
}

// ───────────────────────── Engine 装配 ─────────────────────────

/// 依书源取页模式 + 浏览器探测,构建合适的 [`Engine`]。
///
/// - `http.fetcher == reqwest` → 纯 reqwest(撞挑战即降级);
/// - 否则且探测到系统浏览器 → 升级式取页(撞挑战时经 UI 授权后解挑战);
/// - 否则 → 纯 reqwest。
///
/// 是否真的开浏览器由 `TuiBrowserUi::authorize` 在撞挑战时把关(设置开关 ∨ 首次弹窗)。
pub fn build_engine(source: BookSource) -> parse_book_source::Result<Engine> {
    // 登录态(loginHeader/cookies)注入每个请求;加载时做 TTL 清理(过期清登录态)。
    // 空态(未登录 / 无需登录的书源)→ 注入为空,行为与现状一致(向后兼容)。
    let state = crate::cache::load_source_state(&source.url);
    let engine = if matches!(source.http.fetcher, FetchMode::Reqwest) {
        Engine::new(source)?
    } else {
        let mut opts = BrowserOptions::default();
        if let Ok(dir) = crate::utils::novel_catch_dir() {
            opts.profile_dir = dir.join("browser-profile");
        }
        opts.total_timeout = Duration::from_secs(90);
        opts.ui = browser_ui();
        match BrowserFetcher::detect(opts) {
            Some(browser) => Engine::with_browser_assist(source, Some(browser))?,
            None => Engine::new(source)?,
        }
    };
    Ok(engine
        .with_login_header(state.login_header)
        .with_cookies(&state.cookies))
}
