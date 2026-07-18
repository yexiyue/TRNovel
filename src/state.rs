//! 进程级全局原子(ambient 单例):外观、阅读显示偏好与 TTS 模型句柄。
//!
//! 单实例 TUI 的真单例、无退出存档需求 → 用 ratatui-kit 的 `Atom`(module-level static,
//! `use_atom(&ATOM)` 细粒度订阅、跨页面持久;组件外可经 `Atom::get`/`Atom::set` 直接读写)。
//!
//! 带 `Drop::save` 兜底存档的缓存(History / BookSourceCache / TTSConfig)**不**放这里——
//! Rust 的 `static` 析构永不运行,atom 化会丢退出兜底存档,仍由 `App` `use_state` 持有。
//! 浏览器验证提示 atom 见 [`crate::browser_assist::BROWSER_PROMPT`],与其消费方就近放置。

use novel_tts::NovelTTS;
use ratatui_kit::Atom;

use crate::{AppearanceConfig, ReaderDisplayConfig};

/// TUI 外观配置:驱动根 `PaletteProvider`,主题设置页写入并落盘。
pub static APPEARANCE: Atom<AppearanceConfig> = Atom::new(AppearanceConfig::default);

/// 阅读显示偏好:与配色外观独立,避免主题切换携带阅读行为状态。
pub static READER_DISPLAY: Atom<ReaderDisplayConfig> = Atom::new(ReaderDisplayConfig::default);

/// 已加载的 TTS 模型句柄:阅读页加载后跨页面保留(`None` = 未加载)。
pub static NOVEL_TTS: Atom<Option<NovelTTS>> = Atom::new(|| None);

/// 全应用键位表:启动时从 `~/.novel/keybindings.toml` 合并(见 `crate::keymap`),
/// 运行期只读;无配置文件时即内置默认表。
pub static KEYMAP: Atom<crate::keymap::AppKeymap> = Atom::new(crate::keymap::AppKeymap::default);
