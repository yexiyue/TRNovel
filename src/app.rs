use std::{sync::Arc, time::Duration};

use futures::FutureExt;
use ratatui::{style::Style, widgets::Borders};
use ratatui_kit::{
    AnyElement, Context, Hooks, Props, UseAtom, UseFuture, UseState, UseTerminalSize, component,
    element,
    prelude::{Border, ContextProvider, Fragment, PaletteProvider, RouteState, RouterProvider},
    routes,
};
use tokio::sync::Notify;

use crate::{
    AppearanceConfig, History, ReaderDisplayConfig, TRNovel,
    book_source::BookSourceCache,
    components::{Loading, WarningModal},
    errors::Errors,
    novel::{local_novel::LocalNovel, network_novel::NetworkNovel},
    pages::{
        ReadNovel,
        home::Home,
        local_novel::SelectFile,
        network_novel::{
            book_detail::BookDetail, book_source_login::BookSourceLogin,
            book_source_manager::BookSourceManager, select_books::SelectBooks,
        },
        select_history::SelectHistory,
        theme_setting::ThemeSetting,
    },
};
mod layout;
use layout::Layout;

#[derive(Debug, Props)]
pub struct AppProps {
    pub trnovel: TRNovel,
    #[cfg(feature = "tts")]
    pub listening: crate::tts::Handle,
}

#[component]
pub fn App(props: &AppProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    hooks.use_terminal_size();
    let mut loading = hooks.use_state(|| true);
    // App owns caches with Drop saving; listening preferences are confirmed snapshots
    // from the independent worker and have no reader-side persistence hook.
    let history_state = hooks.use_state(|| None::<History>);
    let book_sources_catch_state = hooks.use_state(|| None::<BookSourceCache>);
    #[cfg(feature = "tts")]
    let listening_snapshot = hooks.use_state(crate::tts::Snapshot::default);
    #[cfg(feature = "tts")]
    let listening_context = crate::tts::TtsContext {
        handle: props.listening.clone(),
        snapshot: listening_snapshot,
    };
    #[cfg(feature = "tts")]
    hooks.use_future({
        let mut watched = props.listening.watch();
        async move {
            let mut state = listening_snapshot;
            state.set(watched.borrow().clone());
            while watched.changed().await.is_ok() {
                state.set(watched.borrow().clone());
            }
        }
    });
    let error = hooks.use_state(|| None::<String>);
    // keybindings.toml 的合并告警:非致命,启动后一次性弹出、ESC 关闭即消。
    let keymap_warnings = hooks.use_state(|| None::<String>);

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

            crate::state::APPEARANCE.set(AppearanceConfig::load()?);
            crate::state::READER_DISPLAY.set(ReaderDisplayConfig::load()?);

            // 按键配置:任何问题都降级为告警,不进 Err 路径(不阻断启动)。
            let (keymap, warnings) = crate::keymap::load_keymap();
            crate::state::KEYMAP.set(keymap);
            if !warnings.is_empty() {
                keymap_warnings.write().replace(warnings.join("\n"));
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
            "/book-source-login"=> BookSourceLogin,
            "/select-books"=> SelectBooks,
            "/book-detail"=> BookDetail,
            "/network-novel"=> ReadNovel<NetworkNovel>,
            // 主题设置
            "/theme-setting"=> ThemeSetting,
        }
    );

    let appearance = hooks.use_atom(&crate::state::APPEARANCE);
    let palette = appearance.read().palette();

    if error.read().is_some() {
        element!(
            PaletteProvider(palette: palette){
                Border(style: Style::new().bg(palette.bg), borders: Borders::NONE){
                    WarningModal(
                        tip: error.read().clone().unwrap_or_default(),
                        is_error: error.read().is_some(),
                        open: true,
                    )
                }
            }
        )
        .into_any()
    } else {
        element!(Fragment{
            {if loading.get() {
                element!(Loading(tip:"加载缓存中...")).into_any()
            } else {
                element!(
                    PaletteProvider(palette: palette){
                        Border(style: Style::new().bg(palette.bg), borders: Borders::NONE){
                            ContextProvider(value:Context::owned(history_state)){
                                ContextProvider(value:Context::owned(book_sources_catch_state)){
                                    { {
                                        let router = element!(RouterProvider(
                                            routes: routes,
                                            index_path: "/home",
                                            state: RouteState::new(props.trnovel.clone())
                                        ));
                                        #[cfg(feature = "tts")]
                                        let router = element!(ContextProvider(value:Context::owned(listening_context)){ {router} });
                                        router.into_any()
                                    } }
                                }
                            }
                            WarningModal(
                                tip: keymap_warnings.read().clone().unwrap_or_default(),
                                is_error: false,
                                open: keymap_warnings.read().is_some(),
                                on_close: move |_: ()| { keymap_warnings.write().take(); },
                            )
                        }
                    }
                ).into_any()
            }}
        })
        .into_any()
    }
}
