use super::Callback;
use crate::theme::AppChromeTheme;
use crossterm::event::MouseButton;
use ratatui::layout::Constraint;
use ratatui_kit::prelude::*;
#[derive(Props)]
pub struct ButtonProps {
    pub label: String,
    pub on_press: Callback<()>,
}
#[component]
pub fn SettingsButton(props: &ButtonProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let hovered = hooks.use_hover();
    let on_press = props.on_press.clone();
    hooks.use_pointer(PointerOptions::default(), move |event| {
        if matches!(
            event.kind,
            PointerKind::Click {
                button: MouseButton::Left,
                ..
            }
        ) {
            on_press.call(());
            return EventResult::Consumed;
        }
        EventResult::Ignored
    });
    let text = format!("[{}]", props.label);
    let width = ratatui::text::Line::from(text.as_str()).width() as u16;
    element!(View(width:Constraint::Length(width),height:Constraint::Length(1)){widget(ratatui::text::Line::styled(text,if hovered{theme.selected}else{theme.highlight}))})
}
