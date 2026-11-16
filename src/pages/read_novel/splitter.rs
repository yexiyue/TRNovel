//! Directory/body geometry and the reader's resize handle.
use crate::{cache::TocWidth, state::READER_DISPLAY, theme::AppChromeTheme};
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Rect},
    style::Modifier,
    text::Line,
    widgets::Widget,
};
use ratatui_kit::prelude::*;

pub const HANDLE_WIDTH: u16 = 3;
const MIN_DIRECTORY: u16 = 18;
const MIN_BODY: u16 = 30;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PaneLayout {
    pub directory: u16,
    pub body: u16,
    pub available: u16,
}
impl PaneLayout {
    pub fn new(width: u16, preference: TocWidth) -> Option<Self> {
        let available = width.saturating_sub(HANDLE_WIDTH);
        if available < MIN_DIRECTORY + MIN_BODY {
            return None;
        }
        let directory = preference
            .width(available)
            .clamp(MIN_DIRECTORY, available - MIN_BODY);
        Some(Self {
            directory,
            body: available - directory,
            available,
        })
    }
    fn clamp_directory(self, width: i32) -> u16 {
        width.clamp(
            i32::from(MIN_DIRECTORY),
            i32::from(self.available - MIN_BODY),
        ) as u16
    }
}
#[derive(Clone, Copy)]
struct DragSession {
    pointer: u16,
    directory: u16,
    preference: TocWidth,
}
#[derive(Props)]
pub struct PaneDividerProps {
    pub layout: PaneLayout,
    pub active: bool,
    pub on_feedback: crate::hooks::Callback<bool>,
}
#[component]
pub fn PaneDivider(props: &PaneDividerProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let display = hooks.use_atom(&READER_DISPLAY);
    let mut session = hooks.use_state(|| None::<DragSession>);
    let hovered = hooks.use_hover();
    let active = props.active;
    let layout = props.layout;
    let (width, height) = hooks.use_terminal_size();
    hooks.use_effect(
        move || session.set(None),
        (active, layout.available, width, height),
    );
    hooks.use_drag(move |drag| {
        if !active {
            session.set(None);
            return;
        }
        match drag.phase {
            DragPhase::Start => session.set(Some(DragSession {
                pointer: drag.to.x,
                directory: layout.directory,
                preference: display.read().toc_width,
            })),
            DragPhase::Move | DragPhase::End => {
                if let Some(start) = session.get() {
                    let columns = layout.clamp_directory(
                        i32::from(start.directory) + i32::from(drag.to.x)
                            - i32::from(start.pointer),
                    );
                    let next = if columns == start.directory {
                        start.preference
                    } else {
                        TocWidth::from_columns(columns, layout.available)
                    };
                    let current = display.read().toc_width;
                    if current != next {
                        display.write().toc_width = next;
                    }
                }
                if drag.phase == DragPhase::End {
                    session.set(None);
                }
            }
            DragPhase::Cancel => session.set(None),
        }
    });
    let highlight = active && (hovered || session.get().is_some());
    hooks.use_effect(
        {
            let feedback = props.on_feedback.clone();
            move || feedback.call(highlight)
        },
        highlight,
    );
    hooks.use_on_drop({
        let feedback = props.on_feedback.clone();
        move || feedback.call(false)
    });
    let content = DividerWidget {
        style: if highlight {
            theme.highlight.add_modifier(Modifier::BOLD)
        } else {
            theme.border
        },
    };
    element!(View(width:Constraint::Length(HANDLE_WIDTH),height:Constraint::Fill(1)){widget(content)})
}
#[derive(Clone)]
struct DividerWidget {
    style: ratatui::style::Style,
}
impl Widget for DividerWidget {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        for y in area.y..area.bottom() {
            Line::styled("│", self.style).render(
                Rect::new(area.x + area.width / 2, y, u16::from(area.width > 0), 1),
                buffer,
            );
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn geometry_preserves_both_panes_and_a_separate_handle() {
        assert!(PaneLayout::new(50, TocWidth::default()).is_none());
        for width in 51..300 {
            for percent in [1.0, 32.15, 50.0, 99.0] {
                let layout = PaneLayout::new(width, TocWidth::from_percent(percent)).unwrap();
                assert!(layout.directory >= 18 && layout.body >= 30);
                assert_eq!(layout.directory + layout.body + HANDLE_WIDTH, width);
            }
        }
    }
}
