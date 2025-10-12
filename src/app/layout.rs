use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui_kit::{
    AnyElement, Hooks, State, UseContext, UseEffect, UseEvents, UseExit, UseRouter, component,
    element, prelude::Outlet,
};

use crate::{
    Commands, History, HistoryItem, TRNovel, pages::network_novel::book_detail::BookDetailState,
};

#[component]
pub fn Layout(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut navigate = hooks.use_navigate();
    let mut exit = hooks.use_exit();
    let params = hooks.try_use_route_state::<TRNovel>();
    let history = *hooks.use_context::<State<Option<History>>>();

    hooks.use_effect(
        || {
            if let Some(params) = params.clone() {
                match params.subcommand.clone() {
                    Some(Commands::Network) => {
                        navigate.push("/book-source");
                    }
                    Some(Commands::History) => {
                        navigate.push("/select-history");
                    }
                    Some(Commands::Local { path }) => {
                        if let Some(path) = path {
                            navigate.push_with_state("/select-file", path);
                        } else {
                            navigate.push("/select-file");
                        }
                    }
                    Some(Commands::Quick) => {
                        if let Some(history) = history.read().as_ref()
                            && let Some((path, item)) = history.histories.first()
                        {
                            match item {
                                HistoryItem::Local(_) => {
                                    navigate.push_with_state("/local-novel", path.clone());
                                }
                                HistoryItem::Network(_) => {
                                    navigate.push_with_state(
                                        "/book-detail",
                                        BookDetailState::Cache { url: path.clone() },
                                    );
                                }
                            }
                        }
                    }
                    _ => {}
                };
            }
        },
        params.clone(),
    );

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
