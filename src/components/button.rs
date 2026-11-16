//! Outlined actions shared by dialogs and primary page actions.
use crate::{hooks::use_callback, theme::AppChromeTheme};
use crossterm::event::MouseButton;
use ratatui::{
    layout::{Alignment, Constraint},
    style::Modifier,
    text::Line,
};
use ratatui_kit::prelude::*;

#[derive(Default, Props)]
pub struct OutlinedButtonProps {
    pub label: String,
    pub focused: bool,
    pub disabled: bool,
    pub on_press: Handler<'static, ()>,
}
#[component]
pub fn OutlinedButton(
    props: &mut OutlinedButtonProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let on_press = use_callback(&mut hooks, &mut props.on_press);
    let hovered = hooks.use_hover();
    let disabled = props.disabled;
    hooks.use_pointer(PointerOptions::default(), move |event| {
        if !disabled && matches!(event.kind, PointerKind::Down(MouseButton::Left)) {
            on_press.call(());
            EventResult::Consumed
        } else {
            EventResult::Ignored
        }
    });
    let style = if disabled {
        theme.meta_label.add_modifier(Modifier::DIM)
    } else if props.focused || hovered {
        theme.highlight.add_modifier(Modifier::BOLD)
    } else {
        theme.text
    };
    let width = Line::from(props.label.as_str()).width() as u16 + 6;
    element!(Border(width:Constraint::Length(width),height:Constraint::Length(3),border_style:style){Text(text:props.label.clone(),style:style,alignment:Alignment::Center)})
}
