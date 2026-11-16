use super::{
    Edit, Editor, Focus, Row, use_area,
    view::{self, Hit},
};
use crate::theme::AppChromeTheme;
use crossterm::event::MouseButton;
use ratatui::layout::Constraint;
use ratatui_kit::prelude::*;
use std::sync::Arc;
#[derive(Props)]
pub struct FormProps<K: Clone + Send + Sync + Unpin + 'static> {
    pub rows: Arc<Vec<Row<K>>>,
    pub selected: usize,
    pub focused: bool,
    pub active: bool,
    pub identity: usize,
    pub keyboard_revision: u64,
    pub editor: Editor,
}
#[component]
pub fn SettingsForm<K>(props: &FormProps<K>, mut hooks: Hooks) -> impl Into<AnyElement<'static>>
where
    K: Clone + Send + Sync + Unpin + 'static,
{
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let geometry = use_area(&mut hooks);
    let area = *geometry.lock().unwrap_or_else(|e| e.into_inner());
    let list = Arc::new(view::entries(&props.rows));
    let rows = props.rows.clone();
    let active = props.active;
    let mut offset = hooks.use_state(|| 0usize);
    let cursor = props.selected.min(rows.len().saturating_sub(1));
    hooks.use_effect(
        {
            let list = list.clone();
            move || {
                if area.height > 0 {
                    let next = view::reveal(&list, cursor, offset.get(), usize::from(area.height));
                    if next != offset.get() {
                        offset.set(next);
                    }
                }
            }
        },
        (
            props.identity,
            cursor,
            area.height,
            props.focused,
            props.keyboard_revision,
            list.len(),
        ),
    );
    let hover = hooks.use_hover_row({
        let geometry = geometry.clone();
        let list = list.clone();
        let rows = rows.clone();
        move |pos| {
            let area = *geometry.lock().unwrap_or_else(|e| e.into_inner());
            if active {
                view::hit(area, &list, offset.get(), &rows, pos)
            } else {
                None
            }
        }
    });
    let tip = hover.get().filter(|_| active).and_then(|hit| {
        let row = rows.get(hit.row())?;
        let controls = view::control_area(area, row.control);
        let clipped =
            ratatui::text::Line::from(row.value.as_str()).width() > usize::from(controls.width);
        if row.detail.is_empty() || (row.detail == row.value && !clipped) || row.detail == row.label
        {
            return None;
        }
        let line = list
            .iter()
            .position(|entry| matches!(entry, view::Entry::Field(index) if *index == hit.row()))?;
        let y = area.y + u16::try_from(line.checked_sub(offset.get())?).ok()?;
        if y >= area.bottom() {
            return None;
        }
        let anchor = match hit {
            Hit::Row(_) => ratatui::layout::Rect::new(area.x + 2, y, 1, 1),
            _ => view::control_area(
                ratatui::layout::Rect::new(area.x, y, area.width, 1),
                row.control,
            ),
        };
        Some(crate::components::tooltip::Hint {
            text: row.detail.clone(),
            shortcut: String::new(),
            anchor,
        })
    });
    crate::components::tooltip::use_tooltip(&mut hooks, tip);
    hooks.use_scrollbar(
        ScrollbarOptions {
            content_length: list.len(),
            viewport_length: usize::from(area.height),
            position: offset.get(),
            scrollbar: crate::theme::scrollbar_style(theme.border, theme.title),
            placement: TrackPlacement::RightInward(0),
        },
        move |line| offset.set(line),
    );
    hooks.use_pointer(PointerOptions::default(), {
        let list = list.clone();
        let rows = rows.clone();
        let editor = props.editor.clone();
        move |event| {
            if !active {
                return EventResult::Ignored;
            }
            match event.kind {
                PointerKind::Click {
                    button: MouseButton::Left,
                    ..
                } => {
                    if let Some(hit) =
                        view::hit(event.area, &list, offset.get(), &rows, event.position)
                    {
                        let handler = &editor;
                        handler.call(Edit::Focus(hit.row()));
                        match hit {
                            Hit::Decrease(i) => handler.call(Edit::Adjust(i, -1)),
                            Hit::Increase(i) => handler.call(Edit::Adjust(i, 1)),
                            Hit::Activate(i) => handler.call(Edit::Activate(i)),
                            Hit::Row(_) => {}
                        }
                        return EventResult::Consumed;
                    }
                }
                PointerKind::ScrollDown => {
                    offset.set(
                        (offset.get() + 3)
                            .min(list.len().saturating_sub(usize::from(event.area.height))),
                    );
                    return EventResult::Consumed;
                }
                PointerKind::ScrollUp => {
                    offset.set(offset.get().saturating_sub(3));
                    return EventResult::Consumed;
                }
                _ => {}
            }
            EventResult::Ignored
        }
    });
    let content = view::FormWidget {
        area: geometry,
        rows,
        list,
        cursor,
        focus: if props.focused {
            Focus::Form
        } else {
            Focus::Tabs
        },
        offset: offset.get(),
        hover: if active { hover.get() } else { None },
        theme,
    };
    element!(View(height:Constraint::Fill(1)){widget(content)})
}
