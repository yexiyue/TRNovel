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

/// 面板条目数(索引上界)。新增条目时同步改这里、下方渲染的索引判断与 `PANEL_HEIGHT`。
const ITEM_COUNT: usize = 3 + cfg!(feature = "tts") as usize;

/// 面板高度:每个条目 3 行(含边框),外加浮层自身的边框与外边距 4 行。
/// 不够高会把最后一个条目裁掉 —— 条目数变了必须跟着改。
const PANEL_HEIGHT: u16 = ITEM_COUNT as u16 * 3 + 5;

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

    let hint = Line::from(format!(
        "↑/↓ 选择 · ←/→ 调整 · {} 关闭",
        display_first_key(&reader_keymap, ReaderAction::ToggleReaderSettings)
    ))
    .centered()
    .style(theme.muted);

    let config = *reader_display.read();
    // 同屏展示派生步长:用户无法凭「2」想象效果,但看到「每页滚动 28 行」就懂了;
    // 终端极矮把步长压到 1 时也能一眼看见。
    let overlap_value = format!(
        "{} 行(每页滚动 {} 行)",
        config.page_overlap,
        config.page_step(
            props
                .viewport_lines
                .unwrap_or_else(|| visible_lines(height))
        )
    );
    let title_value = if config.show_title { "开" } else { "关" };
    let spacing_value = if config.paragraph_spacing {
        "开"
    } else {
        "关"
    };

    element!(Modal(
        width:Constraint::Percentage(60),
        height: Constraint::Length(PANEL_HEIGHT),
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
                View(height: Constraint::Length(3)) {
                    AdjustableSettingItem(
                        is_editing: index.get() == 0 && is_editing,
                        label: "翻页重叠:".to_string(),
                        value: overlap_value,
                        on_decrease: move |_| reader_display.write().decrease_page_overlap(),
                        on_increase: move |_| reader_display.write().increase_page_overlap(),
                        on_click: focus_row(index, 0),
                    )
                }
                View(height: Constraint::Length(3)) {
                    AdjustableSettingItem(
                        is_editing: index.get() == 1 && is_editing,
                        // 此前只有 v 键盲切、没有可视入口,与翻页重叠同属 ReaderDisplayConfig,
                        // 一并收进面板。
                        label: "显示标题:".to_string(),
                        value: title_value.to_string(),
                        on_decrease: move |_| reader_display.write().show_title = false,
                        on_increase: move |_| reader_display.write().show_title = true,
                        on_click: focus_row(index, 1),
                    )
                }
                View(height: Constraint::Length(3)) {
                    AdjustableSettingItem(
                        is_editing: index.get() == 2 && is_editing,
                        // 关掉后段落首尾相接,一屏正文行数近乎翻倍 —— 小屏用户的诉求。
                        label: "段落间距:".to_string(),
                        value: spacing_value.to_string(),
                        on_decrease: move |_| reader_display.write().paragraph_spacing = false,
                        on_increase: move |_| reader_display.write().paragraph_spacing = true,
                        on_click: focus_row(index, 2),
                    )
                }
                Fragment {
                    {follow_setting(reader_display, index, index.get() == 3 && is_editing)}
                }
            }
        }
    })
    .into_any()
}

#[cfg(feature = "tts")]
fn follow_setting(
    config: AtomState<crate::ReaderDisplayConfig>,
    index: ratatui_kit::State<usize>,
    editing: bool,
) -> AnyElement<'static> {
    let value = if config.read().follow_tts {
        "开"
    } else {
        "关"
    };
    element!(View(height: Constraint::Length(3)) {
        AdjustableSettingItem(is_editing: editing, label: "跟随朗读:".to_string(), value: value.to_string(),
            on_decrease: move |_| config.write().follow_tts = false,
            on_increase: move |_| config.write().follow_tts = true,
            on_click: focus_row(index, 3))
    }).into_any()
}
#[cfg(not(feature = "tts"))]
fn follow_setting(
    _: AtomState<crate::ReaderDisplayConfig>,
    _: ratatui_kit::State<usize>,
    _: bool,
) -> AnyElement<'static> {
    element!(Fragment).into_any()
}
