//! 阅读设置浮层:把阅读行为偏好(`ReaderDisplayConfig`)搬到 TUI 里可视化调整,
//! 用户不必手改 `~/.trnovel/config.toml` 的 reader 分节。
//!
//! 只改 `READER_DISPLAY` atom,落盘由 `ReadNovel` 的防抖 effect 统一负责——面板与
//! `v` 键(切换标题)走同一条写盘路径,不会出现两套时序。

use crossterm::event::{Event, KeyCode, KeyEventKind, MouseEventKind};
use ratatui::{
    layout::{Constraint, Margin},
    style::Style,
    text::Line,
};
use ratatui_kit::prelude::*;

use crate::{
    components::{AdjustableSettingItem, setting_item::focus_row},
    keymap::{ReaderAction, display_first_key},
    pages::read_novel::visible_lines,
    theme::AppChromeTheme,
};

/// 浮层按「常用在前」的顺序列出全部排版项;窗口高度不够时滚动显示。
/// 值是 describe_setting / adjust 的条目序号 —— 两处表格保持完整条目,
/// 浮层只是决定展示顺序,免得同一份文案写两遍。
#[cfg(feature = "tts")]
pub(crate) const SETTING_ROWS: &[usize] = &[
    4,
    5,
    3,
    2,
    6,
    7,
    8,
    GLOBAL_HINT_ROW,
    BLANK_ROW,
    TOC_ROW,
    9,
    0,
    1,
    10,
];
#[cfg(not(feature = "tts"))]
pub(crate) const SETTING_ROWS: &[usize] = &[
    4,
    5,
    3,
    2,
    6,
    7,
    8,
    GLOBAL_HINT_ROW,
    BLANK_ROW,
    TOC_ROW,
    9,
    0,
    1,
];

/// 「目录占宽」的条目序号:追加在现有条目末尾,避免打乱浮层已用的行号。
/// tts 构建比非 tts 多「跟随朗读」一行,故用 cfg 偏移。
pub(crate) const TOC_ROW: usize = 10 + cfg!(feature = "tts") as usize;

/// 整屏最底部(非阅读页)的全局快捷键提示开关。
/// 与阅读页提示分开,才能只留一种提示。
pub(crate) const GLOBAL_HINT_ROW: usize = TOC_ROW + 1;

/// 章末翻页留白开关,默认关:最后一页贴底。
pub(crate) const BLANK_ROW: usize = TOC_ROW + 2;

/// 浮层条目数(索引上界)。
const ITEM_COUNT: usize = SETTING_ROWS.len();

/// 浮层每个条目 3 行(含边框),自身再占 5 行(边框 + 外边距);
/// 可见行数按终端高度算,放不下的条目滚动显示。
fn visible_rows(height: u16) -> usize {
    (usize::from(height.saturating_sub(7) / 3)).clamp(1, ITEM_COUNT)
}

#[derive(Props, Default)]
pub struct ReaderSettingsModalProps {
    pub open: bool,
    pub layer: Option<InputLayer>,
    pub on_close: Handler<'static, ()>,
    pub viewport_lines: Option<usize>,
    pub is_editing: bool,
}

#[component]
pub fn ReaderSettingsModal(
    props: &mut ReaderSettingsModalProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let reader_keymap = hooks.use_atom(&crate::state::KEYMAP).read().reader.clone();
    let reader_display = hooks.use_atom(&crate::state::READER_DISPLAY);
    // 阅读页传入扣除工具栏后的实际正文视口。独立挂载时保留终端高度回退。
    let (_, height) = hooks.use_terminal_size();

    let mut index = hooks.use_state(|| 0usize);
    let is_open = props.open;
    let is_editing = props.is_editing;
    // These handlers belong to the modal even though they are registered above its subtree.
    let own_layer = hooks.use_input_layer(is_open && props.layer.is_none(), true);
    let layer = props.layer.unwrap_or(own_layer);
    let scope = EventScope::Layer(layer);

    hooks.use_event_handler(scope, EventPriority::Normal, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        if !is_open || !is_editing {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                index.set((index.get() + 1).min(ITEM_COUNT - 1));
                EventResult::Consumed
            }
            KeyCode::Char('k') | KeyCode::Up => {
                index.set(index.get().saturating_sub(1));
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });

    // 鼠标滚轮移动面板焦点:面板开着时整屏滚轮都归面板并消费(背景正文此时
    // is_scroll=false 不响应;消费可避免「浮层没关、正文偷偷滚」)。
    hooks.use_event_handler_with_options(
        scope,
        EventPriority::Low,
        EventOptions { hit_test: false },
        {
            let mut index = index;
            move |event| {
                if !is_open {
                    return EventResult::Ignored;
                }
                let Event::Mouse(m) = event else {
                    return EventResult::Ignored;
                };
                match m.kind {
                    MouseEventKind::ScrollDown => {
                        index.set((index.get() + 1).min(ITEM_COUNT - 1));
                        EventResult::Consumed
                    }
                    MouseEventKind::ScrollUp => {
                        index.set(index.get().saturating_sub(1));
                        EventResult::Consumed
                    }
                    _ => EventResult::Ignored,
                }
            }
        },
    );

    // hooks 全部注册完再提前返回:关闭时不构建面板内容(提示行的 describe + format!
    // 与整棵子树每帧都会被求值一次,而它绝大多数时间不可见)。
    if !is_open {
        return element!(View).into_any();
    }

    let config = *reader_display.read();
    // 面板自身边框底部的操作说明也归「界面提示」总开关管,关掉后只留边框。
    let hint = if config.show_hint_bar {
        Line::from(format!(
            "↑/↓ 选择 · ←/→ 调整 · {} 关闭 · 全部设置见首页「全局设置」",
            display_first_key(&reader_keymap, ReaderAction::ToggleReaderSettings)
        ))
        .centered()
        .style(theme.muted)
    } else {
        Line::from("")
    };
    let rows = visible_rows(height);
    // 整页翻:光标越过分页边界时窗口跟着跳一页,和列表页的翻页手感一致。
    let offset = if ITEM_COUNT > rows {
        ((index.get() / rows) * rows).min(ITEM_COUNT - rows)
    } else {
        0
    };
    // 同屏展示派生步长:用户无法凭「2」想象效果,但看到「每页滚动 28 行」就懂了;
    // 终端极矮把步长压到 1 时也能一眼看见。
    let visible = props
        .viewport_lines
        .unwrap_or_else(|| visible_lines(height, &config));
    let page_step = config.page_step(visible);

    element!(Modal(
        width:Constraint::Percentage(70),
        height: Constraint::Length((rows * 3 + 5) as u16),
        open: is_open,
        layer: layer,
        margin: Margin::new(1, 1),
        style: Style::default().dim(),
    ) {
        View(margin: Margin::new(1, 1)) {
            if !props.on_close.is_default() {
                crate::components::ActionBar {
                    crate::components::ActionButton(label: "关闭阅读设置", shortcut: "Esc", on_press: props.on_close.take())
                }
            }
            Border(
                border_style: theme.border.not_dim(),
                top_title: Line::from("阅读设置").centered().style(theme.title),
                bottom_title: hint,
            ) {
                // 单列窗口:按终端高度显示尽可能多的条目,剩下的用翻页窗口滚动。
                // 双列时代每列只剩 36 列,「2 行(每页滚动 19 行)」会被裁掉尾巴。
                View {
                    for row in 0..rows {
                        {setting_row(reader_display, index, offset + row, is_editing, &config, page_step)}
                    }
                }
            }
        }
    })
    .into_any()
}

/// 渲染单个条目。值文案在这里现算(配置每次写入都会触发重渲染),回调只报方向,
/// 钳制交给 `ReaderDisplayConfig` 的增减方法 —— 面板不重复实现边界。
fn setting_row(
    display: AtomState<crate::ReaderDisplayConfig>,
    index: ratatui_kit::State<usize>,
    row: usize,
    is_editing: bool,
    config: &crate::ReaderDisplayConfig,
    page_step: usize,
) -> AnyElement<'static> {
    // row 是浮层里的行号(0..4),SETTING_ROWS 把它翻译成设置项序号。
    let item = SETTING_ROWS[row];
    let (label, value) = describe_setting(config, item, page_step);
    element!(View(height: Constraint::Length(3)) {
        AdjustableSettingItem(
            is_editing: is_editing && index.get() == row,
            label: label,
            value: value,
            on_decrease: move |_| adjust(display, item, -1),
            on_increase: move |_| adjust(display, item, 1),
            on_click: focus_row(index, row),
        )
    })
    .into_any()
}

/// 条目序号 → (标签, 当前值文案)。新增条目只改这里和 `adjust`,不必再动渲染树。
pub(crate) fn describe_setting(
    config: &crate::ReaderDisplayConfig,
    row: usize,
    page_step: usize,
) -> (String, String) {
    let on_off = |value: bool| if value { "开" } else { "关" }.to_string();
    match row {
        // 用户无法凭「2」想象效果,但看到「每页滚动 28 行」就懂了;终端极矮把
        // 步长压到 1 时也能一眼看见。
        0 => (
            "翻页重叠:".to_string(),
            format!("{} 行(每页滚动 {} 行)", config.page_overlap, page_step),
        ),
        1 => ("显示标题:".to_string(), on_off(config.show_title)),
        2 => (
            "段间空行:".to_string(),
            if config.paragraph_gap == 0 {
                "0 行(段落相接)".to_string()
            } else {
                format!("{} 行", config.paragraph_gap)
            },
        ),
        3 => (
            "首行缩进:".to_string(),
            if config.first_line_indent == 0 {
                "0 字(不缩进)".to_string()
            } else {
                format!("{} 字", config.first_line_indent)
            },
        ),
        4 => (
            "每行字数:".to_string(),
            if config.lines_per_row == 0 {
                "跟随窗口".to_string()
            } else {
                // 一个汉字两列:写清列数,用户不必自己换算「30 字为什么这么窄」。
                format!(
                    "{} 字({} 列)",
                    config.lines_per_row,
                    config.lines_per_row * 2
                )
            },
        ),
        5 => (
            "左右留白:".to_string(),
            if config.h_margin == 0 {
                "0 列(贴边)".to_string()
            } else {
                format!("{} 列", config.h_margin)
            },
        ),
        6 => ("边框:".to_string(), config.frameline.label().to_string()),
        7 => ("边框标题:".to_string(), on_off(config.show_frameline_title)),
        8 => ("框底提示:".to_string(), on_off(config.show_hint_bar)),
        9 => ("↑/↓ 翻页:".to_string(), on_off(config.arrows_page)),
        GLOBAL_HINT_ROW => ("页底提示:".to_string(), on_off(config.show_global_hint)),
        BLANK_ROW => ("翻页留白:".to_string(), on_off(config.allow_page_blank)),
        #[cfg(feature = "tts")]
        10 => ("跟随朗读:".to_string(), on_off(config.follow_tts)),
        TOC_ROW => (
            "目录占宽:".to_string(),
            config.toc_ratio.label().to_string(),
        ),
        _ => (String::new(), String::new()),
    }
}

/// 加减一个单位。方向用 i32 而非 bool,是因为多数条目真的有多档(0..=4 行空行),
/// 只有开关型才把它折成真假。
pub(crate) fn adjust(display: AtomState<crate::ReaderDisplayConfig>, row: usize, delta: i32) {
    let mut config = display.write();
    match row {
        0 => {
            if delta > 0 {
                config.increase_page_overlap();
            } else {
                config.decrease_page_overlap();
            }
        }
        1 => config.show_title = delta > 0,
        2 => {
            if delta > 0 {
                config.increase_paragraph_gap();
            } else {
                config.decrease_paragraph_gap();
            }
        }
        3 => {
            if delta > 0 {
                config.increase_first_line_indent();
            } else {
                config.decrease_first_line_indent();
            }
        }
        4 => {
            if delta > 0 {
                config.increase_lines_per_row();
            } else {
                config.decrease_lines_per_row();
            }
        }
        5 => {
            if delta > 0 {
                config.increase_h_margin();
            } else {
                config.decrease_h_margin();
            }
        }
        6 => {
            config.frameline = if delta > 0 {
                config.frameline.next()
            } else {
                config.frameline.previous()
            };
        }
        7 => config.show_frameline_title = delta > 0,
        8 => config.show_hint_bar = delta > 0,
        9 => config.arrows_page = delta > 0,
        GLOBAL_HINT_ROW => config.show_global_hint = delta > 0,
        BLANK_ROW => config.allow_page_blank = delta > 0,
        #[cfg(feature = "tts")]
        10 => config.follow_tts = delta > 0,
        TOC_ROW => config.toc_ratio = config.toc_ratio.shift(if delta > 0 { 1 } else { -1 }),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setting_rows_cover_every_item_exactly_once() {
        let mut rows = SETTING_ROWS.to_vec();
        rows.sort_unstable();
        assert_eq!(rows, (0..SETTING_ROWS.len()).collect::<Vec<_>>());
    }

    #[test]
    fn every_setting_has_a_label_and_value() {
        let config = crate::ReaderDisplayConfig::default();
        for (index, row) in SETTING_ROWS.iter().enumerate() {
            let (label, value) = describe_setting(&config, *row, 28);
            assert!(!label.is_empty(), "条目 {index} 没有标签");
            assert!(!value.is_empty(), "条目 {index} 没有值文案");
        }
    }
}
