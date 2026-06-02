//! 浏览器辅助验证的交互模态(反爬):
//! - 撞挑战需授权时弹「本次 / 总是 / 拒绝」;
//! - 出现 Turnstile 勾选框时提示「请去浏览器点确认」,可 Esc 取消(→降级)。
//!
//! 状态由全局上下文 `State<Option<BrowserPrompt>>` 承载,`crate::browser_assist` 的内部 UI
//! 从解挑战的异步任务里写入,本组件消费并把用户选择回送。

use crate::browser_assist::BrowserPrompt;
use crate::hooks::UseThemeConfig;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use parse_book_source::AuthDecision;
use ratatui::{
    layout::{Alignment, Constraint, Margin},
    style::{Style, Stylize},
    text::Line,
};
use ratatui_kit::{
    AnyElement, Hooks, State, UseContext, UseEvents, component, element,
    prelude::{Border, Modal, Text, View},
};
use std::sync::atomic::Ordering;

#[component]
pub fn BrowserPromptModal(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_theme_config();
    let state = *hooks.use_context::<State<Option<BrowserPrompt>>>();

    hooks.use_events(move |event: Event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            // 先只读判定当前变体(读守卫随即释放,避免与下面的写冲突)。
            let variant = match state.read().as_ref() {
                Some(BrowserPrompt::Authorize { .. }) => 1u8,
                Some(BrowserPrompt::Click { .. }) => 2u8,
                None => return,
            };
            match (variant, key.code) {
                (1, KeyCode::Char('y') | KeyCode::Char('Y')) => {
                    respond(state, AuthDecision::Once, false)
                }
                (1, KeyCode::Char('a') | KeyCode::Char('A')) => {
                    respond(state, AuthDecision::Always, true)
                }
                (1, KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc) => {
                    respond(state, AuthDecision::Deny, false)
                }
                (2, KeyCode::Esc) => cancel_click(state),
                _ => {}
            }
        }
    });

    let (open, title, content) = match state.read().as_ref() {
        Some(BrowserPrompt::Authorize { source_name, .. }) => (
            true,
            "需要浏览器验证".to_string(),
            format!(
                "书源「{source_name}」遇到人机验证。\n\n是否打开系统浏览器协助通过?\n\n[Y] 本次允许    [A] 总是允许    [N] 拒绝"
            ),
        ),
        Some(BrowserPrompt::Click { .. }) => (
            true,
            "请在浏览器中确认".to_string(),
            "请在弹出的浏览器窗口里点一下「确认您是真人」。\n完成后会自动继续。\n\n[Esc] 取消"
                .to_string(),
        ),
        None => (false, String::new(), String::new()),
    };

    element!(Modal(
        open: open,
        width: Constraint::Percentage(60),
        height: Constraint::Length(11),
        style: Style::default().dim()
    ){
        Border(
            border_style: theme.warning_modal.border,
            top_title: Some(Line::from(title).style(theme.warning_modal.border_title).centered())
        ){
            View(margin: Margin::new(2, 2)){
                Text(
                    text: content,
                    style: theme.warning_modal.text,
                    alignment: Alignment::Center,
                )
            }
        }
    })
}

/// 记录授权决定并撤下弹窗。`persist_always` 时把「总是允许」落盘(走文件,B 键可关);
/// 否则把「本次 / 拒绝」记入本会话缓存。`authorize` 的轮询会随即取到。
fn respond(state: State<Option<BrowserPrompt>>, decision: AuthDecision, persist_always: bool) {
    if persist_always {
        let _ = crate::browser_assist::set_always_allowed(true);
    } else {
        crate::browser_assist::record_decision(decision);
    }
    *state.write() = None;
}

/// 取消解挑战(置取消标志 → 解挑战循环随即中止降级)。
fn cancel_click(state: State<Option<BrowserPrompt>>) {
    if let Some(BrowserPrompt::Click { cancel, .. }) = state.read().clone() {
        cancel.store(true, Ordering::Relaxed);
    }
    *state.write() = None;
}
