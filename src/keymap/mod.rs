//! 阅读页按键映射:action 枚举、代码内默认表与 `~/.novel/keybindings.toml` 加载。
//!
//! 基于 [`ratatui_kit_keymap`]:一个 `Keymap<A>` 即一个 scope,当前只有 `reader`
//! (阅读页),后续 scope(shell 键/列表页)由后续变更逐步扩入。配置文件对程序
//! **只读**,加载/合并的任何问题都降级为告警(不阻断启动)。

use std::sync::Arc;

use ratatui_kit_keymap::{Keymap, KeymapWarning};
use serde::{Deserialize, Serialize};

/// 阅读页(`read_novel` 子树)的语义按键 action。
///
/// serde 变体名(snake_case)即 `keybindings.toml` 里的配置键名 —— 对用户是
/// 稳定契约,改名属破坏性变更(旧配置会降级为「未知操作」告警)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReaderAction {
    ScrollUp,
    ScrollDown,
    PrevChapter,
    NextChapter,
    PageUp,
    PageDown,
    GoTop,
    GoBottom,
    VolumeUp,
    VolumeDown,
    TogglePlay,
    ToggleTitle,
    ToggleReadMode,
    ToggleInfo,
    ToggleTts,
}

/// 全应用键位表。挂 `Arc` 供 `use_keymap_handler` 每帧克隆(引用计数,非深拷贝)。
#[derive(Debug, Clone)]
pub struct AppKeymap {
    pub reader: Arc<Keymap<ReaderAction>>,
}

impl Default for AppKeymap {
    fn default() -> Self {
        Self {
            reader: Arc::new(reader_defaults()),
        }
    }
}

/// 默认表逐键对照迁移前的硬编码键位(含 i/I、t/T 的大小写双绑定;方向键绑在
/// 首位,帮助/提示取首键显示时保持「↑ / ↓」的既有视觉习惯)。
fn reader_defaults() -> Keymap<ReaderAction> {
    Keymap::builder()
        .bind(ReaderAction::ScrollUp, ["up", "k"])
        .desc(ReaderAction::ScrollUp, "向上滚动(章首连按翻上一章)")
        .bind(ReaderAction::ScrollDown, ["down", "j"])
        .desc(ReaderAction::ScrollDown, "向下滚动(章末连按翻下一章)")
        .bind(ReaderAction::PrevChapter, ["left", "h"])
        .desc(ReaderAction::PrevChapter, "上一章(章首)")
        .bind(ReaderAction::NextChapter, ["right", "l"])
        .desc(ReaderAction::NextChapter, "下一章")
        .bind(ReaderAction::PageUp, ["pageup"])
        .desc(ReaderAction::PageUp, "上一页")
        .bind(ReaderAction::PageDown, ["pagedown"])
        .desc(ReaderAction::PageDown, "下一页")
        .bind(ReaderAction::GoTop, ["home"])
        .desc(ReaderAction::GoTop, "跳到开头")
        .bind(ReaderAction::GoBottom, ["end"])
        .desc(ReaderAction::GoBottom, "跳到结尾")
        .bind(ReaderAction::VolumeUp, ["+"])
        .desc(ReaderAction::VolumeUp, "增大音量")
        .bind(ReaderAction::VolumeDown, ["-"])
        .desc(ReaderAction::VolumeDown, "减小音量")
        .bind(ReaderAction::TogglePlay, ["p"])
        .desc(ReaderAction::TogglePlay, "播放/暂停")
        .bind(ReaderAction::ToggleTitle, ["v"])
        .desc(ReaderAction::ToggleTitle, "隐藏/显示标题")
        .bind(ReaderAction::ToggleReadMode, ["tab"])
        .desc(ReaderAction::ToggleReadMode, "切换章节选择/阅读模式")
        .bind(ReaderAction::ToggleInfo, ["i", "I"])
        .desc(ReaderAction::ToggleInfo, "打开/关闭快捷键帮助")
        .bind(ReaderAction::ToggleTts, ["t", "T"])
        .desc(ReaderAction::ToggleTts, "打开/关闭TTS设置")
        .build()
}

/// 加载 `~/.novel/keybindings.toml` 并合并到默认表。
///
/// 返回 `(键位表, 用户可读的中文告警)`。文件不存在 → 默认、无告警;读取失败或
/// 整体不是合法 TOML → 默认 + 一条告警;条目级问题(非法键位/类型错误/冲突/
/// 未知操作)由 crate 降级为逐条告警。
pub fn load_keymap() -> (AppKeymap, Vec<String>) {
    let mut keymap = AppKeymap::default();
    let Ok(dir) = crate::utils::novel_catch_dir() else {
        return (keymap, Vec::new());
    };
    let path = dir.join("keybindings.toml");
    if !path.exists() {
        return (keymap, Vec::new());
    }
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            return (
                keymap,
                vec![format!("keybindings.toml 读取失败:{e},已使用默认按键")],
            );
        }
    };
    let table: ratatui_kit_keymap::toml::Table = match ratatui_kit_keymap::toml::from_str(&text) {
        Ok(table) => table,
        Err(e) => {
            return (
                keymap,
                vec![format!(
                    "keybindings.toml 不是合法 TOML:{e}\n已使用默认按键"
                )],
            );
        }
    };
    let mut messages = Vec::new();
    if let Some(reader_table) = table.get("reader").and_then(|v| v.as_table()).cloned() {
        let mut reader = (*keymap.reader).clone();
        messages.extend(
            reader
                .merge_toml_table(reader_table)
                .iter()
                .map(render_warning),
        );
        keymap.reader = Arc::new(reader);
    }
    (keymap, messages)
}

/// 把 crate 的结构化告警渲染成中文(枚举是 `non_exhaustive`,新变体走英文兜底)。
fn render_warning(warning: &KeymapWarning) -> String {
    match warning {
        KeymapWarning::ParseError { action, input, .. } => {
            format!("[reader] {action} 的键位「{input}」无法解析,已回退默认键位")
        }
        KeymapWarning::InvalidEntry { action, .. } => {
            format!("[reader] {action} 的值类型不对(应为键位字符串或字符串列表),已回退默认键位")
        }
        KeymapWarning::Conflict { key, actions, .. } => {
            format!(
                "[reader] 键 \"{key}\" 被绑定到多个操作({}),冲突的自定义已回退默认",
                actions.join("、")
            )
        }
        KeymapWarning::UnknownAction { name, .. } => {
            format!("[reader] 未知操作 \"{name}\",该条已忽略")
        }
        other => other.to_string(),
    }
}

/// 按项目显示习惯渲染 action 的当前键名("↑ / K" 风格:方向键用箭头、单字符大写)。
/// 帮助浮层与底部提示都从这里取,保证显示的永远是实际生效的绑定。
pub fn display_keys(keymap: &Keymap<ReaderAction>, action: ReaderAction) -> String {
    // 美化后去重:小写 t 与 shift-t 都显示为「T」,双绑定只出现一次(同迁移前)。
    let mut names: Vec<String> = Vec::new();
    for name in keymap.describe(action) {
        let pretty = prettify(&name);
        if !names.contains(&pretty) {
            names.push(pretty);
        }
    }
    names.join(" / ")
}

/// action 当前首个键的显示名(供底部提示等只放得下一个键的场合;空绑定返回 "?")。
pub fn display_first_key(keymap: &Keymap<ReaderAction>, action: ReaderAction) -> String {
    keymap
        .describe(action)
        .first()
        .map(|name| prettify(name))
        .unwrap_or_else(|| "?".to_string())
}

/// 单个键名的显示美化:沿用迁移前帮助浮层的视觉语言(单字母大写、方向键箭头;
/// `Shift-t` 这类单字母 shift 组合折叠为大写字母,与其小写绑定显示合一)。
fn prettify(name: &str) -> String {
    if let Some(ch) = name.strip_prefix("Shift-")
        && ch.chars().count() == 1
    {
        return ch.to_uppercase();
    }
    match name {
        "Up" => "↑".to_string(),
        "Down" => "↓".to_string(),
        "Left" => "←".to_string(),
        "Right" => "→".to_string(),
        "Hyphen" => "-".to_string(),
        name if name.chars().count() == 1 => name.to_uppercase(),
        name => name.to_string(),
    }
}

#[cfg(test)]
mod tests;
