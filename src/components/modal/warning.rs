use crossterm::event::KeyCode;
use ratatui::{layout::Constraint, style::Style, text::Line, widgets::Padding};
use ratatui_kit::{
    AnyElement, Handler, Hooks, Props, UseExit, UseTheme, component, element, prelude::AlertModal,
};

use crate::theme::AppChromeTheme;

#[derive(Props, Default)]
pub struct WarningModalProps {
    pub tip: String,
    pub is_error: bool,
    pub open: bool,
    pub on_close: Handler<'static, ()>,
}

/// 告警/错误弹窗项目 wrapper:按 `is_error` 分支设置关闭键与提示。
/// 保留「错误且无 on_close → 退出程序」语义。
#[component]
pub fn WarningModal(
    props: &mut WarningModalProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let mut exit = hooks.use_exit();
    let is_error = props.is_error;
    let mut on_close = props.on_close.take();

    element!(AlertModal(
        open: props.open,
        width: Constraint::Percentage(50),
        height: Constraint::Length(6),
        style: Style::new().dim(),
        padding: Padding::uniform(1),
        title: Line::from(if is_error { "错误" } else { "警告" }),
        title_style: if is_error { theme.error } else { theme.title },
        border_style: if is_error { theme.error } else { theme.title },
        message: props.tip.clone(),
        close_hint: Line::from(if is_error { "按q退出" } else { "按ESC键继续" })
            .style(theme.muted)
            .centered(),
        close_keys: if is_error {
            vec![KeyCode::Char('q')]
        } else {
            vec![KeyCode::Esc]
        },
        on_close: move |_: ()| {
            if is_error && on_close.is_default() {
                exit();
            } else {
                on_close(());
            }
        },
    ))
}
