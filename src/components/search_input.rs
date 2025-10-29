use crate::hooks::UseThemeConfig;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::Constraint,
    style::{Style, Stylize},
    text::Line,
};
use ratatui_kit::prelude::*;
use std::ops::Deref;
use tui_input::backend::crossterm::EventHandler;

#[derive(Default, Props)]
pub struct SearchInputProps {
    pub value: String,
    pub placeholder: String,
    pub validate: Handler<'static, String, (bool, String)>,
    pub on_submit: Handler<'static, String, bool>,
    pub clear_on_submit: bool,
    pub clear_on_escape: bool,
    pub is_editing: bool,
    pub on_clear: Handler<'static, ()>,
    pub on_editing_change: Handler<'static, bool>,
}

#[component]
pub fn SearchInput(
    props: &mut SearchInputProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let is_editing = props.is_editing;

    let mut value = hooks.use_state(tui_input::Input::default);
    let mut is_valid = hooks.use_state(|| None::<bool>);
    let mut validate_fn = props.validate.take();
    let mut status_message = hooks.use_state(String::new);
    let theme = hooks.use_theme_config();

    let clear_on_submit = props.clear_on_submit;
    let clear_on_escape = props.clear_on_escape;
    let mut on_submit = props.on_submit.take();
    let mut on_clear = props.on_clear.take();
    let mut on_editing_change = props.on_editing_change.take();

    hooks.use_effect(
        || {
            let new_value = value.read().clone().with_value(props.value.clone());
            value.set(new_value);
        },
        props.value.clone(),
    );

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            if is_editing {
                match key.code {
                    KeyCode::Esc => {
                        if clear_on_escape {
                            value.write().reset();
                            is_valid.set(None);
                            status_message.set(String::new());
                            on_clear(());
                        }
                        on_editing_change(false);
                    }
                    KeyCode::Enter => {
                        if !on_submit.is_default() {
                            let valid = on_submit(value.read().value().to_string());
                            if valid && clear_on_submit {
                                value.write().reset();
                                is_valid.set(None);
                                status_message.set(String::new());
                                on_clear(());
                            }
                            if valid {
                                on_editing_change(false);
                            }
                        }
                    }
                    _ => {
                        value.write().handle_event(&event);

                        if !validate_fn.is_default() {
                            let (valid, message) = validate_fn(value.read().value().to_string());
                            is_valid.set(Some(valid));
                            status_message.set(message);
                        }
                    }
                }
            } else if let KeyCode::Char('s') = key.code {
                on_editing_change(true);
            }
        }
    });

    element!(
        Border(
            height:Constraint::Length(3),
            border_style: if let Some(valid)=is_valid.get() && is_editing{
                        if valid {
                            theme.search.success_border
                        } else {
                            theme.search.error_border
                        }
                    } else {
                       theme.basic.border
                    },
            top_title: if let Some(valid)=is_valid.get() && !status_message.read().is_empty() && is_editing{
                if valid {
                    Some(Line::from(status_message.read().deref().to_string()).style(theme.search.success_border_info))
                } else {
                    Some(Line::from(status_message.read().deref().to_string()).style(theme.search.error_border_info))
                }
            } else {
                None
            },
        ){
            Input(
                input: value.read().clone(),
                cursor_style: Style::new().on_dark_gray(),
                style: theme.search.text,
                placeholder: props.placeholder.clone(),
                placeholder_style: theme.search.placeholder,
                hide_cursor: !is_editing,
            )
        }
    )
}
