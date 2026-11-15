use super::{Callback, use_area, view};
use crate::theme::AppChromeTheme;
use crossterm::event::{Event, KeyCode, KeyEventKind, MouseButton, MouseEventKind};
use ratatui::layout::{Constraint, Margin, Position, Rect};
use ratatui_kit::prelude::*;
use std::sync::Arc;
#[derive(Props)]
pub struct ChoiceListProps {
    pub colors: Option<Arc<Vec<ratatui::style::Color>>>,
    pub title: String,
    pub options: Arc<Vec<String>>,
    pub initial: usize,
    pub on_select: Callback<usize>,
    pub on_cancel: Callback<()>,
    pub dismiss_outside: bool,
}
#[component]
pub fn SettingsChoiceList(
    props: &ChoiceListProps,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let geometry = use_area(&mut hooks);
    let area = *geometry.lock().unwrap_or_else(|e| e.into_inner());
    let count = props.options.len();
    let last = count.saturating_sub(1);
    let initial = props.initial.min(last);
    let mut selected = hooks.use_state(move || initial);
    let mut offset = hooks.use_state(|| 0usize);
    hooks.use_effect(
        move || {
            selected.set(initial);
            offset.set(0);
        },
        (props.options.clone(), initial),
    );
    hooks.use_effect(
        move || {
            let height = usize::from(area.height.saturating_sub(2)).max(1);
            let value = selected.get();
            let current = offset.get();
            let next = if value < current {
                value
            } else if value >= current + height {
                value + 1 - height
            } else {
                current
            };
            offset.set(next.min(count.saturating_sub(height)));
        },
        (selected.get(), count, area.height),
    );
    let hover = hooks.use_hover_row({
        let geometry = geometry.clone();
        move |pos| {
            let area = *geometry.lock().unwrap_or_else(|e| e.into_inner());
            choice_at(area, pos, count, offset.get())
        }
    });
    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, {
        let commit = props.on_select.clone();
        let cancel = props.on_cancel.clone();
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press {
                return EventResult::Ignored;
            }
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => selected.set((selected.get() + 1).min(last)),
                KeyCode::Up | KeyCode::Char('k') => selected.set(selected.get().saturating_sub(1)),
                KeyCode::Home => selected.set(0),
                KeyCode::End => selected.set(last),
                KeyCode::PageDown => selected.set((selected.get() + 5).min(last)),
                KeyCode::PageUp => selected.set(selected.get().saturating_sub(5)),
                KeyCode::Enter | KeyCode::Char(' ') => {
                    if count > 0 {
                        commit.call(selected.get());
                    }
                }
                KeyCode::Esc => cancel.call(()),
                _ => return EventResult::Ignored,
            }
            EventResult::Consumed
        }
    });
    hooks.use_scrollbar(
        ScrollbarOptions {
            content_length: count,
            viewport_length: usize::from(area.height.saturating_sub(2)),
            position: offset.get(),
            scrollbar: crate::theme::scrollbar_style(theme.border, theme.title),
            ..Default::default()
        },
        move |value| offset.set(value),
    );
    hooks.use_pointer(PointerOptions::default(), {
        let commit = props.on_select.clone();
        move |event| {
            match event.kind {
                PointerKind::Click {
                    button: MouseButton::Left,
                    ..
                } => {
                    if let Some(index) = choice_at(event.area, event.position, count, offset.get())
                    {
                        commit.call(index);
                        return EventResult::Consumed;
                    }
                }
                PointerKind::ScrollDown => {
                    selected.set((selected.get() + 1).min(last));
                    return EventResult::Consumed;
                }
                PointerKind::ScrollUp => {
                    selected.set(selected.get().saturating_sub(1));
                    return EventResult::Consumed;
                }
                _ => {}
            }
            EventResult::Ignored
        }
    });
    hooks.use_event_handler_with_options(
        EventScope::Current,
        EventPriority::Low,
        EventOptions { hit_test: false },
        {
            let geometry = geometry.clone();
            let cancel = props.on_cancel.clone();
            let dismiss = props.dismiss_outside;
            move |event| {
                if dismiss
                    && let Event::Mouse(mouse) = event
                    && mouse.kind == MouseEventKind::Down(MouseButton::Left)
                    && !geometry
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .contains(Position::new(mouse.column, mouse.row))
                {
                    cancel.call(());
                    return EventResult::Consumed;
                }
                EventResult::Ignored
            }
        },
    );
    let content = view::PickerWidget {
        colors: props.colors.clone(),
        area: geometry,
        title: props.title.clone(),
        options: props.options.as_ref().clone(),
        selected: selected.get(),
        offset: offset.get(),
        hover: hover.get(),
        theme,
    };
    element!(View{widget(content)})
}
pub fn choice_at(area: Rect, pos: Position, count: usize, offset: usize) -> Option<usize> {
    let inner = area.inner(Margin::new(1, 1));
    if !inner.contains(pos) {
        return None;
    }
    let index = offset + usize::from(pos.y - inner.y);
    (index < count).then_some(index)
}

#[derive(Props)]
pub struct PickerProps {
    pub colors: Option<Arc<Vec<ratatui::style::Color>>>,
    pub title: String,
    pub options: Arc<Vec<String>>,
    pub initial: usize,
    pub on_select: Callback<usize>,
    pub on_cancel: Callback<()>,
}
#[component]
pub fn SettingsPicker(props: &PickerProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let layer = hooks.use_input_layer(true, true);
    let (_, height) = hooks.use_terminal_size();
    let rows = (props.options.len() as u16 + 2)
        .min(height.saturating_sub(2))
        .max(3);
    element!(Modal(open:true,layer:Some(layer),width:Constraint::Length(40),height:Constraint::Length(rows)){
        SettingsChoiceList(..ChoiceListProps{colors:props.colors.clone(),title:props.title.clone(),options:props.options.clone(),initial:props.initial,on_select:props.on_select.clone(),on_cancel:props.on_cancel.clone(),dismiss_outside:true})
    })
}
