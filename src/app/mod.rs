use std::fs::File;

use ratatui_kit::{
    AnyElement, Context, Hooks, Props, UseFuture, UseState, UseTerminalSize, component, element,
    prelude::{ContextProvider, Fragment, RouterProvider},
    routes,
};

pub mod state;
pub use state::State;

use crate::{
    History, TRNovel, ThemeConfig,
    book_source::BookSourceCache,
    components::{Loading2, WarningModal},
    errors::Errors,
    pages::{
        home::Home, local_novel::SelectFile2, playground::Playground,
        select_history2::SelectHistory2,
    },
    utils::novel_catch_dir,
};

#[derive(Debug, Props)]
pub struct AppProps {
    pub trnovel: TRNovel,
}

#[component]
pub fn App(_props: &AppProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    hooks.use_terminal_size();
    let mut loading = hooks.use_state(|| true);
    let mut theme_config_state = hooks.use_state(ThemeConfig::default);
    let history_state = hooks.use_state(|| None::<History>);
    let book_sources_catch_state = hooks.use_state(|| None::<BookSourceCache>);
    let error = hooks.use_state(|| None::<String>);

    hooks.use_future(async move {
        loading.set(true);
        tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
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

        loading.set(false);
    });

    let routes = routes!(
        "/"=>Home,
        "/playground"=>Playground,
        "/select-history"=>SelectHistory2,
        "/select-file"=>SelectFile2,
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
                element!(Loading2(tip:"加载缓存中...")).into_any()
            } else {
                element!(
                    ContextProvider(value:Context::owned(theme_config_state)){
                        ContextProvider(value:Context::owned(history_state)){
                            ContextProvider(value:Context::owned(book_sources_catch_state)){
                                RouterProvider(
                                    routes:routes,
                                    index_path:"/select-file",
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
