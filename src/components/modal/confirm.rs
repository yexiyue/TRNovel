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

/// 确认弹窗项目 wrapper:委托框架 `ConfirmModal` 的独占输入层,保留中文按钮文案。
#[component]
pub fn ConfirmModal(
    props: &mut ConfirmModalProps,
    _hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    element!(KitConfirmModal(
        open: props.open,
        width: Constraint::Percentage(50),
        height: Constraint::Length(10),
        style: Style::default().dim(),
        title: Line::from(props.title.clone()),
        content: props.content.clone(),
        confirm_text: "确认".to_string(),
        cancel_text: "取消".to_string(),
        on_confirm: props.on_confirm.take(),
        on_cancel: props.on_cancel.take(),
    ))
}
