use ratatui_kit::{
    AnyElement, Context, Hooks, Props, UseFuture, UseState, component, element,
    prelude::{ContextProvider, Fragment, RouterProvider},
    routes,
};

pub mod state;
pub use state::State;

use crate::{
    History, TRNovel,
    book_source::BookSourceCache,
    components::{Loading2, WarningModal},
    errors::Errors,
    pages::home::Home,
};

#[derive(Debug, Props)]
pub struct AppProps {
    pub trnovel: TRNovel,
}

#[component]
pub fn App(_props: &AppProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut loading = hooks.use_state(|| true);
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
                    ContextProvider(value:Context::owned(history_state)){
                        ContextProvider(value:Context::owned(book_sources_catch_state)){
                            RouterProvider(
                                routes:routes,
                                index_path:"/"
                            )
                        }
                    }
                ).into_any()
            })
        })
        .into_any()
    }
}
