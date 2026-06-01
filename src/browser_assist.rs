//! 浏览器辅助取页的 app 侧装配(反爬)。
//!
//! 依「书源取页模式 ∧ 用户授权 ∧ 系统浏览器探测」三者交集,决定是否给 `Engine`
//! 接入升级式取页(撞 Cloudflare 挑战 → 系统浏览器解出 `cf_clearance` → 交回 reqwest)。
//! 见 OpenSpec change `browser-fetcher` 的 design D8/D12。

use std::time::Duration;

use parse_book_source::{BookSource, BrowserFetcher, BrowserOptions, Engine, FetchMode};

/// 用户是否已授权浏览器辅助验证。
///
/// 读 `~/.novel/browser_assist.on` 标记;**默认未授权**(spec-safe:不擅自打开用户浏览器)。
/// TODO(首次询问 UX):后续接 TUI 首次弹窗「本次 / 总是 / 拒绝」并写入该标记。
pub fn authorized() -> bool {
    crate::utils::novel_catch_dir()
        .map(|d| d.join("browser_assist.on").exists())
        .unwrap_or(false)
}

/// 浏览器 profile 目录落在 `~/.novel/browser-profile`(养号 + 跨会话缓存 `cf_clearance`)。
fn browser_options() -> BrowserOptions {
    let mut opts = BrowserOptions::default();
    if let Ok(dir) = crate::utils::novel_catch_dir() {
        opts.profile_dir = dir.join("browser-profile");
    }
    opts.total_timeout = Duration::from_secs(90);
    opts
}

/// 依书源取页模式 + 用户授权 + 浏览器探测,构建合适的 [`Engine`]。
///
/// - `http.fetcher == reqwest` → 纯 reqwest(撞挑战即降级);
/// - 否则且已授权且探测到系统浏览器 → 升级式取页(cookie 烤箱);
/// - 否则 → 纯 reqwest。
pub fn build_engine(source: BookSource) -> parse_book_source::Result<Engine> {
    let allow = !matches!(source.http.fetcher, FetchMode::Reqwest) && authorized();
    if allow && let Some(browser) = BrowserFetcher::detect(browser_options()) {
        Engine::with_browser_assist(source, Some(browser))
    } else {
        Engine::new(source)
    }
}
