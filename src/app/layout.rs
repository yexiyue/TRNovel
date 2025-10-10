use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui_kit::{
    AnyElement, Hooks, UseEvents, UseExit, UseRouter, component, element, prelude::Outlet,
};

#[component]
pub fn Layout(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut navigate = hooks.use_navigate();
    let mut exit = hooks.use_exit();

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('q') | KeyCode::Char('Q') => {
                    exit();
                }
                KeyCode::Char('g') | KeyCode::Char('G') => {
                    navigate.go(1);
                }
                KeyCode::Char('b') | KeyCode::Char('B') => {
                    navigate.go(-1);
                }
                _ => {}
            }
        }
    });
    element!(Outlet)
}
