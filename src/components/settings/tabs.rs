use super::{Callback, Focus, use_area, view};
use crate::theme::AppChromeTheme;
use crossterm::event::MouseButton;
use ratatui::layout::Constraint;
use ratatui_kit::prelude::*;
use std::sync::Arc;
#[derive(Props)]
pub struct TabsProps {
    pub labels: Arc<Vec<String>>,
    pub selected: usize,
    pub focused: bool,
    pub active: bool,
    pub on_select: Callback<usize>,
}
#[component]
pub fn SettingsTabs(props: &TabsProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let geometry = use_area(&mut hooks);
    let active = props.active;
    let count = props.labels.len();
    let hover = hooks.use_hover_row({
        let geometry = geometry.clone();
        move |pos| {
            let area = *geometry.lock().unwrap_or_else(|e| e.into_inner());
            if active {
                view::tab_at(area, count, pos)
            } else {
                None
            }
        }
    });
    hooks.use_pointer(PointerOptions::default(), {
        let on_select = props.on_select.clone();
        move |event| {
            if active
                && matches!(
                    event.kind,
                    PointerKind::Click {
                        button: MouseButton::Left,
                        ..
                    }
                )
                && let Some(index) = view::tab_at(event.area, count, event.position)
            {
                on_select.call(index);
                return EventResult::Consumed;
            }
            EventResult::Ignored
        }
    });
    let content = view::TabsWidget {
        area: geometry,
        labels: props.labels.clone(),
        section: props.selected,
        focus: if props.focused {
            Focus::Tabs
        } else {
            Focus::Form
        },
        hover: if active { hover.get() } else { None },
        theme,
    };
    element!(View(height:Constraint::Length(2)){widget(content)})
}
