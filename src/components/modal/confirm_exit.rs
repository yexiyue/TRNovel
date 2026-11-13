use std::sync::{Arc, Mutex};

use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::{
    layout::{Alignment, Constraint, Direction, Flex, Margin},
    style::{Modifier, Style},
    text::Line,
};
use ratatui_kit::{
    AnyElement, Handler, Hooks, PointerKind, PointerOptions, Props, UseEffect, UseEventHandler,
    UseHover, UseInputLayer, UsePointer, UseState, UseTheme, component, element,
    prelude::{Border, EventOptions, EventPriority, EventResult, EventScope, Modal, Text, View},
};

use crate::theme::AppChromeTheme;

/// 弹窗内的键盘焦点。默认落在「取消」,让回车不会误退出。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    NoRemind,
    Cancel,
    Exit,
}

impl Focus {
    /// 焦点环:勾选项在上,两个按钮在下,与视觉顺序一致。
    const ORDER: [Self; 3] = [Self::NoRemind, Self::Cancel, Self::Exit];

    fn next(self) -> Self {
        let index = Self::ORDER
            .iter()
            .position(|item| *item == self)
            .unwrap_or(1);
        Self::ORDER[(index + 1) % Self::ORDER.len()]
    }

    fn previous(self) -> Self {
        let index = Self::ORDER
            .iter()
            .position(|item| *item == self)
            .unwrap_or(1);
        Self::ORDER[(index + Self::ORDER.len() - 1) % Self::ORDER.len()]
    }
}

#[derive(Props, Default)]
pub struct ConfirmExitModalProps {
    pub open: bool,
    /// 参数是「不再提醒」是否被勾选:为 true 时调用方负责落盘 confirm_exit = false。
    pub on_confirm: Handler<'static, bool>,
    pub on_cancel: Handler<'static, ()>,
}

/// 退出二次确认弹窗。
///
/// 直接复用框架 `Modal` 自带的独占输入层(`use_input_layer` + `layer: Some(..)`):
/// 打开期间下层 Q/G/B 等 shell 快捷键被截断,不需要在 Layout 里自己写按键拦截。
/// 与框架 `ConfirmModal` 的区别只有「不再提醒」勾选项和可循环的三态焦点。
#[component]
pub fn ConfirmExitModal(
    props: &mut ConfirmExitModalProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let open = props.open;
    let mut focus = hooks.use_state(|| Focus::Cancel);
    let mut no_remind = hooks.use_state(|| false);

    // 每次重新打开都回到默认焦点,并清掉上一次的勾选 —— 否则这次取消后下次再开
    // 会带着旧的「不再提醒」,用户没注意到就永久关掉了确认。
    hooks.use_effect(
        move || {
            if open {
                focus.set(Focus::Cancel);
                no_remind.set(false);
            }
        },
        open,
    );

    let layer = hooks.use_input_layer(open, true);
    let on_confirm = Arc::new(Mutex::new(props.on_confirm.take()));
    let on_cancel = Arc::new(Mutex::new(props.on_cancel.take()));

    // 键盘处理器必须把所有 Key 事件消费掉:模态层已 blocks_lower,但同层若有其它
    // 处理器,放行会让它们也响应。鼠标事件一律 Ignored,交给按钮/勾选项/遮罩。
    hooks.use_event_handler(EventScope::Layer(layer), EventPriority::High, {
        let on_confirm = on_confirm.clone();
        let on_cancel = on_cancel.clone();
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press {
                return EventResult::Consumed;
            }
            match key.code {
                KeyCode::Left | KeyCode::Up | KeyCode::BackTab => {
                    focus.set(focus.get().previous());
                }
                KeyCode::Right | KeyCode::Down | KeyCode::Tab => {
                    focus.set(focus.get().next());
                }
                // 空格是勾选框的通用快捷键,不必先把焦点移过去。
                KeyCode::Char(' ') => no_remind.set(!no_remind.get()),
                KeyCode::Enter => match focus.get() {
                    Focus::NoRemind => no_remind.set(!no_remind.get()),
                    Focus::Cancel => (on_cancel.lock().unwrap())(()),
                    Focus::Exit => (on_confirm.lock().unwrap())(no_remind.get()),
                },
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    (on_confirm.lock().unwrap())(no_remind.get())
                }
                // Q 在这里只当作「取消」:用户刚按了一次 Q,不能让同一个键直接退出。
                KeyCode::Esc
                | KeyCode::Char('n')
                | KeyCode::Char('N')
                | KeyCode::Char('q')
                | KeyCode::Char('Q') => (on_cancel.lock().unwrap())(()),
                _ => {}
            }
            EventResult::Consumed
        }
    });

    // 遮罩/面板空白处的左键 = 取消(按钮和勾选项先用 Normal 级指针消费自己的区域)。
    hooks.use_event_handler_with_options(
        EventScope::Layer(layer),
        EventPriority::Low,
        EventOptions { hit_test: false },
        {
            let on_cancel = on_cancel.clone();
            move |event| {
                if let Event::Mouse(m) = event
                    && m.kind == MouseEventKind::Down(MouseButton::Left)
                {
                    (on_cancel.lock().unwrap())(());
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
        },
    );

    let focused = focus.get();
    let checked = no_remind.get();
    let cancel_text = "取消".to_string();
    let exit_text = "退出".to_string();
    let confirm_on_press = confirm_handler(&on_confirm, checked);
    let cancel_on_press = unit_handler(&on_cancel);

    element!(Modal(
        open: open,
        layer: Some(layer),
        width: Constraint::Percentage(50),
        height: Constraint::Length(12),
        style: Style::default().dim(),
    ) {
        Border(
            border_style: theme.border,
            top_title: Some(Line::from("退出").style(theme.title).centered()),
            bottom_title: Some(
                Line::from("←/→ 选择 · 空格切换勾选 · Enter 确认 · Esc 取消")
                    .style(theme.muted)
                    .centered(),
            ),
        ) {
            View(margin: Margin::new(2, 1), flex_direction: Direction::Vertical) {
                View(height: Constraint::Length(1)) {
                    Text(text: "确认退出？", style: theme.text, alignment: Alignment::Center, wrap: true)
                }
                View(height: Constraint::Length(1)) {
                    Text(text: "未保存的阅读进度会自动保存", style: theme.muted, alignment: Alignment::Center, wrap: true)
                }
                View(height: Constraint::Length(1)) {}
                NoRemindRow(
                    checked: checked,
                    focused: focused == Focus::NoRemind,
                    on_toggle: {
                        let mut no_remind = no_remind;
                        move |_: ()| no_remind.set(!no_remind.get())
                    },
                )
                View(height: Constraint::Length(1)) {}
                View(
                    height: Constraint::Length(3),
                    flex_direction: Direction::Horizontal,
                    gap: 3,
                    justify_content: Flex::Center,
                ) {
                    ExitButton(
                        label: cancel_text,
                        focused: focused == Focus::Cancel,
                        on_press: cancel_on_press,
                    )
                    ExitButton(
                        label: exit_text,
                        focused: focused == Focus::Exit,
                        on_press: confirm_on_press,
                    )
                }
            }
        }
    })
}

/// 把共享回包成可复制给子组件的无参 Handler;勾选状态在渲染时已捕获进闭包。
fn confirm_handler(
    shared: &Arc<Mutex<Handler<'static, bool>>>,
    no_remind: bool,
) -> Handler<'static, ()> {
    let shared = Arc::clone(shared);
    Handler::from(move |()| (shared.lock().unwrap())(no_remind))
}

fn unit_handler(shared: &Arc<Mutex<Handler<'static, ()>>>) -> Handler<'static, ()> {
    let shared = Arc::clone(shared);
    Handler::from(move |()| (shared.lock().unwrap())(()))
}

#[derive(Props, Default)]
struct ExitButtonProps {
    label: String,
    focused: bool,
    on_press: Handler<'static, ()>,
}

/// 确认框按钮:风格与框架 ConfirmModal 一致(边框 + 文字,选中反色加粗)。
#[component]
fn ExitButton(props: &mut ExitButtonProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let mut on_press = props.on_press.take();
    let hovered = hooks.use_hover();
    hooks.use_pointer(PointerOptions::default(), move |event| {
        if let PointerKind::Down(MouseButton::Left) = event.kind {
            on_press(());
            EventResult::Consumed
        } else {
            EventResult::Ignored
        }
    });

    let selected = props.focused || hovered;
    let style = if selected {
        theme.highlight.add_modifier(Modifier::BOLD)
    } else {
        theme.text
    };
    let label = if selected {
        format!(" {} ", props.label)
    } else {
        props.label.clone()
    };
    let width = Line::from(props.label.as_str()).width() as u16 + 6;
    element!(Border(
        width: Constraint::Length(width),
        height: Constraint::Length(3),
        border_style: style,
    ) {
        Text(text: Line::styled(label, style), alignment: Alignment::Center)
    })
}

#[derive(Props, Default)]
struct NoRemindRowProps {
    checked: bool,
    focused: bool,
    on_toggle: Handler<'static, ()>,
}

/// 「不再提醒」勾选行;整行可点,悬停高亮。
#[component]
fn NoRemindRow(props: &mut NoRemindRowProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let mut on_toggle = props.on_toggle.take();
    let hovered = hooks.use_hover();
    hooks.use_pointer(PointerOptions::default(), move |event| {
        if let PointerKind::Down(MouseButton::Left) = event.kind {
            on_toggle(());
            EventResult::Consumed
        } else {
            EventResult::Ignored
        }
    });

    let selected = props.focused || hovered;
    let style = if selected {
        theme.highlight.add_modifier(Modifier::BOLD)
    } else {
        theme.text
    };
    let marker = if props.checked { "[x]" } else { "[ ]" };
    element!(View(
        height: Constraint::Length(1),
        flex_direction: Direction::Horizontal,
        justify_content: Flex::Center,
    ) {
        Text(text: format!("{marker} 不再提醒"), style: style, alignment: Alignment::Center)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_cycle_stays_inside_dialog() {
        assert_eq!(Focus::Cancel.next(), Focus::Exit);
        assert_eq!(Focus::Exit.next(), Focus::NoRemind);
        assert_eq!(Focus::NoRemind.next(), Focus::Cancel);
        assert_eq!(Focus::Cancel.previous(), Focus::NoRemind);
    }
}
