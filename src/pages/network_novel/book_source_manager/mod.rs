use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::layout::Direction;
use ratatui_kit::{
    AnyElement, Hooks, State, UseContext, UseEvents, UseState, component, element, prelude::View,
};

mod import_book_source;
mod select_book_source;
use import_book_source::ImportBookSource;
use select_book_source::SelectBookSource;

use crate::book_source::BookSourceCache;

#[component]
pub fn BookSourceManager(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let book_source_cache = *hooks.use_context::<State<Option<BookSourceCache>>>();

    let mut import_mode = hooks.use_state(|| false);
    let mut only_select = hooks.use_state(|| {
        book_source_cache
            .read()
            .as_ref()
            .is_some_and(|c| !c.book_sources.is_empty())
    });
    let is_inputting = hooks.use_state(|| false);

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                // KeyCode::Char('i') | KeyCode::Char('I') => {
                //     info_modal_open.set(!info_modal_open.get());
                // }
                KeyCode::Left | KeyCode::Right => {
                    if !is_inputting.get() {
                        import_mode.set(!import_mode.get());
                    }
                }
                KeyCode::Tab => {
                    only_select.set(!only_select.get());
                    if !only_select.get() {
                        import_mode.set(true);
                    }
                }
                _ => {}
            }
        }
    });

    if only_select.get() {
        element!(View(
            flex_direction:Direction::Horizontal
        ){
            SelectBookSource(
                is_editing: true,
            )
        })
    } else {
        element!(View(
            flex_direction:Direction::Horizontal
        ){
            ImportBookSource(
                is_editing: import_mode.get() || is_inputting.get(),
                is_inputting: is_inputting,
            )
            SelectBookSource(
                is_editing: !import_mode.get() && !is_inputting.get(),
            )
        })
    }
}
