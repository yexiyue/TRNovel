use crate::hooks::UseThemeConfig;
use ratatui::{layout::Constraint, style::Style, text::Line};
use ratatui_kit::{
    AnyElement, Handler, Hooks, Props, component, element, prelude::ConfirmModal as KitConfirmModal,
};

#[derive(Props, Default)]
pub struct ConfirmModalProps {
    pub title: String,
    pub content: String,
    pub open: bool,
    pub on_confirm: Handler<'static, ()>,
    pub on_cancel: Handler<'static, ()>,
}

/// 确认弹窗:薄主题适配层,委托框架 `ConfirmModal`(自带独占输入层 + y/n/Esc/方向键/Enter),
/// 仅把 TRNovel 主题映射成内置所需的 Style props。
#[component]
pub fn ConfirmModal(
    props: &mut ConfirmModalProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_theme_config();

    element!(KitConfirmModal(
        open: props.open,
        width: Constraint::Percentage(50),
        height: Constraint::Length(10),
        style: Style::default().dim(),
        title: Line::from(props.title.clone()),
        content: props.content.clone(),
        confirm_text: "确认".to_string(),
        cancel_text: "取消".to_string(),
        border_style: theme.warning_modal.border,
        title_style: theme.warning_modal.border_title,
        content_style: theme.warning_modal.text,
        selected_button_style: Style::default().fg(theme.colors.primary_color).bold(),
        on_confirm: props.on_confirm.take(),
        on_cancel: props.on_cancel.take(),
    ))
}
