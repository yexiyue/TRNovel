//! 浏览器辅助验证的交互模态(反爬):
//! - 撞挑战需授权时弹「本次 / 总是 / 拒绝」;
//! - 出现 Turnstile 勾选框时提示「请去浏览器点确认」,可 Esc 取消(→降级)。
//!
//! 状态由全局原子 `crate::browser_assist::BROWSER_PROMPT` 承载,`build_engine` 的 `TuiBrowserUi`
//! 从解挑战的异步任务里写入,本组件 `use_atom` 订阅消费并把用户选择回送。

use crate::browser_assist::BrowserPrompt;
use crate::theme::AppChromeTheme;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use parse_book_source::AuthDecision;
use ratatui::{
    layout::{Alignment, Constraint, Margin},
    style::Style,
    text::Line,
};
use ratatui_kit::{
    AnyElement, AtomState, EventPriority, EventResult, EventScope, Hooks, UseAtom, UseEventHandler,
    UseInputLayer, UseTheme, component, element,
    prelude::{Border, Modal, Text, View},
};
use std::sync::atomic::Ordering;

#[component]
pub fn BrowserPromptModal(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let state = hooks.use_atom(&crate::browser_assist::BROWSER_PROMPT);
    let open = state.read().is_some();
    // handler 须与下方 Modal 同处一个独占输入层:Modal open 时自开 blocks_lower 层会截断 root 层,
    // 若 handler 留在 root(EventScope::Current),y/a/n/Esc/Enter 全收不到(模态键盘失聪)。
    let layer = hooks.use_input_layer(open, true);

    hooks.use_event_handler(
        EventScope::Layer(layer),
        EventPriority::Normal,
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            // 先只读判定当前变体(读守卫随即释放,避免与下面的写冲突)。
            let variant = match state.read().as_ref() {
                Some(BrowserPrompt::Authorize { .. }) => 1u8,
                Some(BrowserPrompt::Click { .. }) => 2u8,
                Some(BrowserPrompt::Notice { .. }) => 3u8,
                None => return EventResult::Ignored,
            };
            match (variant, key.code) {
                (1, KeyCode::Char('y') | KeyCode::Char('Y')) => {
                    respond(state, AuthDecision::Once, false);
                    EventResult::Consumed
                }
                (1, KeyCode::Char('a') | KeyCode::Char('A')) => {
                    respond(state, AuthDecision::Always, true);
                    EventResult::Consumed
                }
                (1, KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc) => {
                    respond(state, AuthDecision::Deny, false);
                    EventResult::Consumed
                }
                (2, KeyCode::Esc) => {
                    cancel_click(state);
                    EventResult::Consumed
                }
                (3, KeyCode::Enter | KeyCode::Esc) => {
                    *state.write() = None;
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            }
        },
    );

    let (title, content) = match state.read().as_ref() {
        Some(BrowserPrompt::Authorize { source_name, .. }) => (
            "需要浏览器验证".to_string(),
            format!(
                "书源「{source_name}」遇到人机验证。\n\n是否打开系统浏览器协助通过?\n\n[Y] 本次允许    [A] 总是允许    [N] 拒绝"
            ),
        ),
        Some(BrowserPrompt::Click { .. }) => (
            "请在浏览器中确认".to_string(),
            "请在弹出的浏览器窗口里点一下「确认您是真人」。\n完成后会自动继续。\n\n[Esc] 取消"
                .to_string(),
        ),
        Some(BrowserPrompt::Notice { message }) => (
            "浏览器会话提醒".to_string(),
            format!("{message}\n\n[Enter/Esc] 关闭"),
        ),
        None => (String::new(), String::new()),
    };

    element!(Modal(
        open: open,
        layer: Some(layer),
        width: Constraint::Percentage(60),
        height: Constraint::Length(11),
        style: Style::default().dim()
    ){
        Border(
            border_style: theme.title,
            top_title: Some(Line::from(title).style(theme.title).centered())
        ){
            View(margin: Margin::new(2, 2)){
                Text(
                    text: content,
                    style: theme.text,
                    alignment: Alignment::Center,
                )
            }
        }
    })
}

/// 记录授权决定并撤下弹窗。`persist_always` 时把「总是允许」落盘(走文件,B 键可关);
/// 否则把「本次 / 拒绝」记入本会话缓存。`authorize` 的轮询会随即取到。
fn respond(state: AtomState<Option<BrowserPrompt>>, decision: AuthDecision, persist_always: bool) {
    if persist_always {
        let _ = crate::browser_assist::set_always_allowed(true);
    } else {
        crate::browser_assist::record_decision(decision);
    }
    *state.write() = None;
}

/// 取消解挑战(置取消标志 → 解挑战循环随即中止降级)。
fn cancel_click(state: AtomState<Option<BrowserPrompt>>) {
    if let Some(BrowserPrompt::Click { cancel, .. }) = state.read().clone() {
        cancel.store(true, Ordering::Relaxed);
    }
    *state.write() = None;
}
