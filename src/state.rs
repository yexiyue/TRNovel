//! 进程级全局原子(ambient 单例):主题与 TTS 模型句柄。
//!
//! 单实例 TUI 的真单例、无退出存档需求 → 用 ratatui-kit 0.7 的 `Atom`(module-level static,
//! `use_atom(&ATOM)` 细粒度订阅、跨页面持久;组件外可经 `Atom::get`/`Atom::set` 直接读写)。
//!
//! 带 `Drop::save` 兜底存档的缓存(History / BookSourceCache / TTSConfig)**不**放这里——
//! Rust 的 `static` 析构永不运行,atom 化会丢退出兜底存档,仍由 `App` `use_state` 持有。
//! 浏览器验证提示 atom 见 [`crate::browser_assist::BROWSER_PROMPT`],与其消费方就近放置。

use novel_tts::NovelTTS;
use ratatui_kit::Atom;

use crate::ThemeConfig;

/// 主题配置:`UseThemeConfig`(`hooks/use_theme_token.rs`)订阅它,主题设置页写入并落盘。
pub static THEME: Atom<ThemeConfig> = Atom::new(ThemeConfig::default);

/// 已加载的 TTS 模型句柄:阅读页加载后跨页面保留(`None` = 未加载)。
pub static NOVEL_TTS: Atom<Option<NovelTTS>> = Atom::new(|| None);
