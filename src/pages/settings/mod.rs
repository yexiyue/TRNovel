//! 全局设置页:首页菜单「全局设置」进入,左侧分组、右侧内容。
//!
//! 阅读页的 `o` 浮层只放 4 个高频排版项,其余条目都在这里;两处共用
//! `super::read_novel::settings::{describe_setting, adjust}`,文案与边界钳制只有一份。
//!
//! 本页只写 `READER_DISPLAY` 与路由跳转,**不直接落盘**:阅读偏好的写盘由
//! `ReadNovel` 的防抖 effect 负责,外观由主题设置页经 `ConfigStore` 负责。

use crate::{
    BackgroundMode,
    app::navigation::UseAppNavigate,
    components::{ActionBar, ActionButton, KeyShortcutInfo, ShortcutInfoModal},
    keymap::{ReaderAction, display_keys},
    pages::read_novel::{
        settings::{TOC_ROW, adjust, describe_setting},
        visible_lines,
    },
    theme::{AppChromeTheme, presets::ThemeChoice},
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Direction, Margin},
    text::{Line, Span},
    widgets::Paragraph,
};
use ratatui_kit::prelude::*;
use unicode_width::UnicodeWidthStr;

/// 分组顺序即 Tab 顺序;新增分组只需追加名字并在 `rows_in` 与内容分支里给出条目数。
const SECTIONS: [&str; 5] = ["阅读", "外观", "键位", "数据", "关于"];
const SECTION_READING: usize = 0;
const SECTION_APPEARANCE: usize = 1;
const SECTION_KEYS: usize = 2;
const SECTION_DATA: usize = 3;
const SECTION_ABOUT: usize = 4;

/// 键位分组展示的动作。不是全量 21 个:这里回答「最常用的怎么按」,
/// 全量表仍在阅读页 `I` 帮助里。
const KEY_ITEMS: &[(ReaderAction, &str)] = &[
    (ReaderAction::PageDown, "翻页"),
    (ReaderAction::PageUp, "上一页"),
    (ReaderAction::PrevChapter, "上一章"),
    (ReaderAction::NextChapter, "下一章"),
    (ReaderAction::SearchContent, "搜索正文"),
    (ReaderAction::ToggleReadMode, "目录/正文"),
    (ReaderAction::ToggleReaderSettings, "阅读设置"),
    (ReaderAction::ToggleInfo, "帮助"),
];

/// 阅读分组条目数,与 `describe_setting` 的行号一一对应。
/// 末尾追加了「目录占宽」;tts 构建还多一项「跟随朗读」,行号由 `TOC_ROW` 统一。
fn reading_rows() -> usize {
    TOC_ROW + 1
}

fn rows_in(section: usize) -> usize {
    match section {
        SECTION_READING => reading_rows(),
        SECTION_APPEARANCE => 4,
        SECTION_KEYS => KEY_ITEMS.len(),
        SECTION_DATA => 3,
        SECTION_ABOUT => 4,
        _ => 0,
    }
}

/// 一行「标签 …… 值」:标签左、值右,中间补空格到整行宽度。
///
/// 不用 `Flex::SpaceBetween` 是因为条目会被塞进不同宽度的列里,自己算 padding
/// 才能保证选中行的高亮底色连成一条,不会在中间断开。
fn item_line(
    label: &str,
    value: &str,
    selected: bool,
    width: usize,
    theme: AppChromeTheme,
) -> Line<'static> {
    let prefix = if selected { "▸ " } else { "  " };
    let left = format!("{prefix}{label}");
    let left_width = UnicodeWidthStr::width(left.as_str());
    // 目录路径常常比右栏还宽:保留尾部(文件名/最后一级目录),从左侧裁掉并补省略号,
    // 否则值会顶破整行宽度、把标签列挤得七零八落。
    let budget = width.saturating_sub(left_width + 1);
    let value = truncate_left(value, budget);
    let gap = width
        .saturating_sub(left_width + UnicodeWidthStr::width(value.as_str()))
        .max(1);
    let style = if selected { theme.selected } else { theme.text };
    Line::from(vec![
        Span::styled(left, style),
        Span::styled(" ".repeat(gap), style),
        Span::styled(value, style),
    ])
}

/// 按终端显示宽度从左侧截断,保留尾部;放得下就原样返回。
fn truncate_left(value: &str, budget: usize) -> String {
    if UnicodeWidthStr::width(value) <= budget {
        return value.to_string();
    }
    // 省略号本身占一列,尾部可用的就更少。
    let mut remaining = budget.saturating_sub(1);
    let mut kept: Vec<char> = Vec::new();
    for ch in value.chars().rev() {
        let w = UnicodeWidthStr::width(ch.to_string().as_str());
        if w > remaining {
            break;
        }
        remaining -= w;
        kept.push(ch);
    }
    kept.reverse();
    format!("…{}", kept.into_iter().collect::<String>())
}

/// 只读信息行(数据/关于分组)。
fn info_line(label: &str, value: &str, width: usize, theme: AppChromeTheme) -> Line<'static> {
    item_line(label, value, false, width, theme)
}

#[component]
pub fn Settings(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let (width, height) = hooks.use_terminal_size();
    let appearance = hooks.use_atom(&crate::state::APPEARANCE);
    let reader_display = hooks.use_atom(&crate::state::READER_DISPLAY);
    let reader_keymap = hooks.use_atom(&crate::state::KEYMAP).read().reader.clone();
    // 恢复上次的分组与组内光标;分组数、组内条目数变化时都做边界钳制。
    let restored = crate::app::ui_memory::restored_cursor(crate::app::ui_memory::SETTINGS);
    let initial_section = restored
        .map(|cursor| cursor.index.min(SECTIONS.len() - 1))
        .unwrap_or(SECTION_READING);
    let initial_cursor = restored
        .map(|cursor| {
            cursor
                .offset
                .min(rows_in(initial_section).saturating_sub(1))
        })
        .unwrap_or(0);
    let mut section = hooks.use_state(move || initial_section);
    let mut cursor = hooks.use_state(move || initial_cursor);
    let mut info_open = hooks.use_state(|| false);
    let mut navigate = hooks.use_app_navigate();

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, move |event| {
        // 帮助浮层自己持有输入,别让 Esc/Tab 穿透到分组导航。
        if info_open.get() {
            return EventResult::Ignored;
        }
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Tab => {
                section.set((section.get() + 1) % SECTIONS.len());
                cursor.set(0);
                EventResult::Consumed
            }
            KeyCode::BackTab => {
                let len = SECTIONS.len();
                section.set((section.get() + len - 1) % len);
                cursor.set(0);
                EventResult::Consumed
            }
            KeyCode::Down | KeyCode::Char('j') => {
                cursor.set((cursor.get() + 1).min(rows_in(section.get()).saturating_sub(1)));
                EventResult::Consumed
            }
            KeyCode::Up | KeyCode::Char('k') => {
                cursor.set(cursor.get().saturating_sub(1));
                EventResult::Consumed
            }
            KeyCode::Left | KeyCode::Char('h') if section.get() == SECTION_READING => {
                adjust(reader_display, cursor.get(), -1);
                EventResult::Consumed
            }
            KeyCode::Right | KeyCode::Char('l') if section.get() == SECTION_READING => {
                adjust(reader_display, cursor.get(), 1);
                EventResult::Consumed
            }
            KeyCode::Enter if section.get() == SECTION_APPEARANCE => {
                navigate.push("/theme-setting");
                EventResult::Consumed
            }
            KeyCode::Esc => {
                navigate.go(-1);
                EventResult::Consumed
            }
            KeyCode::Char('i') => {
                info_open.set(true);
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    // 分组或光标变化就写回,离开再回来时恢复。
    let section_now = section.get();
    let cursor_now = cursor.get();
    hooks.use_effect(
        move || {
            crate::app::ui_memory::save_cursor(
                crate::app::ui_memory::SETTINGS,
                crate::app::ui_memory::Cursor::new(section_now, cursor_now),
            );
        },
        (section_now, cursor_now),
    );

    let config = *reader_display.read();
    // 提示条独占整屏最后一行,与阅读页一致:正文视口要先让出这 1 行。
    let hint_rows = u16::from(config.show_hint_bar);
    let page_step = config.page_step(visible_lines(height.saturating_sub(hint_rows), &config));
    // 内容区宽度 = 终端宽 - 边框两侧 - 页边距两侧 - 左侧分组栏。
    let content_width = (width as usize).saturating_sub(2 + 2 + 14);
    let selected = cursor.get();

    let mut lines: Vec<Line<'static>> = Vec::new();
    match section.get() {
        SECTION_READING => {
            for row in 0..reading_rows() {
                let (label, value) = describe_setting(&config, row, page_step);
                let label = label.trim_end_matches(':').to_string();
                lines.push(item_line(
                    &label,
                    &value,
                    row == selected,
                    content_width,
                    theme,
                ));
            }
        }
        SECTION_APPEARANCE => {
            let config = appearance.read();
            let theme_name = ThemeChoice::from_slug(&config.theme_slug)
                .map(|choice| choice.display_name().to_string())
                .unwrap_or_else(|| config.theme_slug.clone());
            let background = match config.background {
                BackgroundMode::Theme => "主题背景",
                BackgroundMode::Terminal => "终端背景",
            };
            let rows = [
                ("主题", theme_name),
                ("背景模式", background.to_string()),
                (
                    "顶部栏",
                    if config.show_header {
                        "显示"
                    } else {
                        "隐藏"
                    }
                    .to_string(),
                ),
                (
                    "退出确认",
                    if config.confirm_exit {
                        "开启"
                    } else {
                        "关闭"
                    }
                    .to_string(),
                ),
            ];
            for (index, (label, value)) in rows.iter().enumerate() {
                lines.push(item_line(
                    label,
                    value,
                    index == selected,
                    content_width,
                    theme,
                ));
            }
            lines.push(Line::from(""));
            lines.push(
                Line::from("  Enter 打开外观与主题设置:配色、自定义方案、背景与顶部栏")
                    .style(theme.muted),
            );
        }
        SECTION_KEYS => {
            for (index, (action, label)) in KEY_ITEMS.iter().enumerate() {
                let keys = display_keys(&reader_keymap, *action);
                lines.push(item_line(
                    label,
                    &keys,
                    index == selected,
                    content_width,
                    theme,
                ));
            }
            lines.push(Line::from(""));
            lines.push(
                Line::from(format!("  自定义键位:编辑 {} 后重启", keybindings_path()))
                    .style(theme.muted),
            );
        }
        SECTION_DATA => {
            for (index, (label, value)) in data_rows().iter().enumerate() {
                if index == selected {
                    lines.push(item_line(label, value, true, content_width, theme));
                } else {
                    lines.push(info_line(label, value, content_width, theme));
                }
            }
        }
        _ => {
            let build = if cfg!(feature = "network") {
                "全功能(本地 + 网络)"
            } else {
                "纯本地(无网络书源)"
            };
            let build = if cfg!(feature = "tts") {
                format!("{build} + 朗读")
            } else {
                build.to_string()
            };
            let rows = [
                ("版本", env!("CARGO_PKG_VERSION").to_string()),
                ("构建", build),
                ("上游仓库", "github.com/yexiyue/TRNovel".to_string()),
                ("协议", "MIT".to_string()),
            ];
            for (index, (label, value)) in rows.iter().enumerate() {
                if index == selected {
                    lines.push(item_line(label, value, true, content_width, theme));
                } else {
                    lines.push(info_line(label, value, content_width, theme));
                }
            }
        }
    }

    let nav: Vec<Line<'static>> = SECTIONS
        .iter()
        .enumerate()
        .map(|(index, name)| {
            if index == section.get() {
                Line::from(format!("▸ {name}")).style(theme.selected)
            } else {
                Line::from(format!("  {name}")).style(theme.text)
            }
        })
        .collect();

    element!(View {
        ActionBar(menu: "全局设置") {
            ActionButton(label: "外观与主题", shortcut: "Enter", on_press: {
                let mut navigate = navigate;
                move |_| navigate.push("/theme-setting")
            })
            ActionButton(label: "帮助", shortcut: "I", on_press: move |_| info_open.set(true))
            ActionButton(label: "返回", shortcut: "Esc", on_press: {
                let mut navigate = navigate;
                move |_| navigate.go(-1)
            })
        }
        Border(
            border_style: theme.border.not_dim(),
            top_title: Line::from("全局设置").style(theme.title).centered(),
            bottom_title: Line::from("Tab 切换分组 · ↑/↓ 选择 · ←/→ 调整 · Enter 打开 · Esc 返回")
                .style(theme.muted)
                .centered(),
        ) {
            View(margin: Margin::new(1, 1)) {
                View(flex_direction: Direction::Horizontal) {
                    View(width: Constraint::Length(14)) {
                        widget(Paragraph::new(nav))
                    }
                    View {
                        widget(Paragraph::new(lines))
                    }
                }
            }
        }
        ShortcutInfoModal(
            key_shortcut_info: KeyShortcutInfo::new(vec![
                ("切换分组", "Tab / Shift+Tab"),
                ("选择条目", "↑ / ↓"),
                ("调整数值", "← / →"),
                ("打开外观与主题", "Enter"),
                ("返回上一页", "Esc"),
            ]),
            open: info_open.get(),
            on_close: {
                let mut open = info_open;
                move |_: ()| open.set(false)
            },
        )
    })
}

/// 数据分组:三处用户可能想打开或备份的目录。
fn data_rows() -> [(&'static str, String); 3] {
    let paths = crate::paths::AppPaths::user_default().ok();
    let show = |path: Option<std::path::PathBuf>| {
        path.map(|path| path.display().to_string())
            .unwrap_or_else(|| "未找到 HOME".to_string())
    };
    [
        (
            "配置",
            show(paths.as_ref().map(crate::paths::AppPaths::config)),
        ),
        (
            "数据",
            show(paths.as_ref().map(crate::paths::AppPaths::data)),
        ),
        (
            "缓存",
            show(paths.as_ref().map(crate::paths::AppPaths::cache)),
        ),
    ]
}

fn keybindings_path() -> String {
    crate::paths::AppPaths::user_default()
        .map(|paths| paths.keybindings().display().to_string())
        .unwrap_or_else(|_| "~/.trnovel/keybindings.toml".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_section_has_rows() {
        for (index, name) in SECTIONS.iter().enumerate() {
            assert!(rows_in(index) > 0, "分组 {name} 没有条目");
        }
    }

    #[test]
    fn item_line_fills_the_whole_width() {
        let theme = AppChromeTheme::default();
        for width in [20, 40, 60] {
            let line = item_line("每行字数", "跟随窗口", true, width, theme);
            assert_eq!(line.width(), width);
        }
    }
}
