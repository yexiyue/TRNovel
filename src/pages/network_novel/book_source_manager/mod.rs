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
use crate::components::modal::shortcut_info_modal::ShortcutInfoModal;

#[component]
pub fn BookSourceManager(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let book_source_cache = *hooks.use_context::<State<Option<BookSourceCache>>>();

    let mut only_select = hooks.use_state(|| {
        book_source_cache
            .read()
            .as_ref()
            .is_some_and(|c| !c.book_sources.is_empty())
    });
    let is_inputting = hooks.use_state(|| false);
    let mut info_modal_open = hooks.use_state(|| false);

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('i') | KeyCode::Char('I') => {
                    info_modal_open.set(!info_modal_open.get());
                }
                KeyCode::Tab => {
                    only_select.set(!only_select.get());
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
                is_editing: !info_modal_open.get(),
            )
            ShortcutInfoModal(
                key_shortcut_info: vec![
                    ("切换导入书源模式", "Tab"),
                    ("删除书源", "D"),
                    ("上下移动", "J / K / ↑ / ↓"),
                    ("选择书源", "Enter"),
                ],
                open: info_modal_open.get(),
            )
        })
    } else {
        element!(View(
            flex_direction:Direction::Horizontal
        ){
            ImportBookSource(
                is_editing: !info_modal_open.get(),
                is_inputting: is_inputting,
            )
            SelectBookSource(
                is_editing: false,
            )
            ShortcutInfoModal(
                key_shortcut_info: vec![
                    ("切换仅选择模式", "Tab"),
                    ("输入书源地址", "S"),
                    ("选择/取消条目", "空格"),
                    ("确认导入/选择", "Enter"),
                    ("上下移动", "J / K / ↑ / ↓"),
                ],
                open: info_modal_open.get(),
            )
        })
    }
}
