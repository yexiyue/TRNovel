use crate::{
    Commands, History, HistoryItem, TRNovel, components::BrowserPromptModal,
    pages::network_novel::book_detail::BookDetailState,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui_kit::{
    AnyElement, EventPriority, EventResult, EventScope, Hooks, State, UseContext, UseEffect,
    UseEventHandler, UseExit, UseRouter, component, element,
    prelude::{Fragment, Outlet},
};
use std::path::PathBuf;

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
                                    navigate.push_with_state("/local-novel", PathBuf::from(path));
                                }
                                HistoryItem::Network(_) => {
                                    navigate.push_with_state(
                                        "/book-detail",
                                        BookDetailState::from_cache(path.clone()),
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

    // 背景 shell 键:注册在 root 层(Current)。当页面的搜索框/模态开启 blocks_lower 输入层时,
    // 本 handler 随之被框架自动截断 —— 这正是旧 `is_inputting` 手动门控的事,现由输入层零竞态完成。
    // 不设 Global:否则会在文本输入时劫持 q/g/b。非自身键返回 Ignored,让 Outlet 子页面继续处理。
    // 优先级用 Low:让页面级 Normal handler 先跑(如书源管理页的 `b` 切换浏览器验证,
    // 与本 shell 的 `b` 后退同键)。q/g 无页面竞争,Low 照常生效。
    hooks.use_event_handler(EventScope::Current, EventPriority::Low, move |event| {
        let Event::Key(key) = event else {
            return EventResult::Ignored;
        };
        if key.kind != KeyEventKind::Press {
            return EventResult::Ignored;
        }
        match key.code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                exit();
                EventResult::Consumed
            }
            KeyCode::Char('g') | KeyCode::Char('G') => {
                navigate.go(1);
                EventResult::Consumed
            }
            KeyCode::Char('b') | KeyCode::Char('B') => {
                navigate.go(-1);
                EventResult::Consumed
            }
            _ => EventResult::Ignored,
        }
    });
    element!(Fragment {
        Outlet
        BrowserPromptModal
    })
}
