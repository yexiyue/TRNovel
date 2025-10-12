use std::{fs::File, sync::Arc, time::Duration};

use futures::FutureExt;
use ratatui_kit::{
    AnyElement, Context, Hooks, Props, UseFuture, UseState, UseTerminalSize, component, element,
    prelude::{ContextProvider, Fragment, RouteState, RouterProvider},
    routes,
};
use tokio::sync::Notify;

use crate::{
    History, TRNovel, ThemeConfig,
    book_source::BookSourceCache,
    components::{Loading, WarningModal},
    errors::Errors,
    novel::{local_novel::LocalNovel, network_novel::NetworkNovel},
    pages::{
        ReadNovel,
        home::Home,
        local_novel::SelectFile,
        network_novel::{
            book_detail::BookDetail, book_source_manager::BookSourceManager,
            select_books::SelectBooks,
        },
        select_history::SelectHistory,
    },
    utils::novel_catch_dir,
};
mod layout;
use layout::Layout;

#[derive(Debug, Props)]
pub struct AppProps {
    pub trnovel: TRNovel,
}

#[component]
pub fn App(props: &AppProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    hooks.use_terminal_size();
    let mut loading = hooks.use_state(|| true);
    let mut theme_config_state = hooks.use_state(ThemeConfig::default);
    let history_state = hooks.use_state(|| None::<History>);
    let book_sources_catch_state = hooks.use_state(|| None::<BookSourceCache>);
    let error = hooks.use_state(|| None::<String>);

    hooks.use_future(async move {
        let notify = Arc::new(Notify::new());
        tokio::spawn({
            let notify = notify.clone();
            async move {
                tokio::time::sleep(Duration::from_millis(200)).await;
                if notify.notified().now_or_never().is_none() {
                    loading.set(true);
                }
            }
        });

        match (move || {
            let history = History::load()?;
            history_state.write().replace(history);

            let book_sources = BookSourceCache::load()?;
            book_sources_catch_state.write().replace(book_sources);

            let path = novel_catch_dir()?.join("theme.json");

            if let Ok(file) = File::open(path) {
                theme_config_state.set(serde_json::from_reader(file).unwrap_or_default());
            }

            Ok::<(), Errors>(())
        })() {
            Ok(_) => {}
            Err(e) => {
                error.write().replace(e.to_string());
            }
        }

        notify.notify_one();
        loading.set(false);
    });

    let routes = routes!(
        "/"=>Layout{
            "/home"=>Home,
            "/select-history"=>SelectHistory,
            // 本地小说
            "/select-file"=> SelectFile,
            "/local-novel"=> ReadNovel<LocalNovel>,
            // 网络小说
            "/book-source"=> BookSourceManager,
            "/select-books"=> SelectBooks,
            "/book-detail"=> BookDetail,
            "/network-novel"=> ReadNovel<NetworkNovel>,
        }
    );

    if error.read().is_some() {
        element!(WarningModal(
            tip: error.read().clone().unwrap_or_default(),
            is_error: error.read().is_some(),
            open: true,
        ))
        .into_any()
    } else {
        element!(Fragment{
            #(if loading.get() {
                element!(Loading(tip:"加载缓存中...")).into_any()
            } else {
                element!(
                    ContextProvider(value:Context::owned(theme_config_state)){
                        ContextProvider(value:Context::owned(history_state)){
                            ContextProvider(value:Context::owned(book_sources_catch_state)){
                                RouterProvider(
                                    routes: routes,
                                    index_path: "/home",
                                    state: RouteState::new(props.trnovel.clone())
                                )
                            }
                        }
                    }
                ).into_any()
            })
        })
        .into_any()
    }
}
