use futures::FutureExt;
use ratatui_kit::{Hooks, State, UseEffect, UseFuture, UseState};
use std::sync::Arc;
use tokio::sync::Notify;

/// 异步加载状态 hook(带 **200ms loading 防抖**)。
///
/// 注意:**不要**改用框架 0.7 内置的 `use_async_state` 替代——后者立刻 `loading.set(true)`,而本 hook
/// 刻意用 `Notify` + `tokio::spawn` 把 loading 延迟 200ms 才显示,以消除快速加载时的 spinner 频闪
/// (见 `dev-notes/knowledge/tui-ratatui-kit.md` 的「狂闪 bug」)。这 200ms 防抖是与内置版的唯一差异、刻意保留。
pub trait UseInitState {
    fn use_init_state<T, E, F>(
        &mut self,
        init_f: F,
    ) -> (State<Option<T>>, State<bool>, State<Option<E>>)
    where
        T: Send + Sync + Unpin,
        E: Send + Sync + Unpin,
        F: Future<Output = Result<T, E>> + Send + 'static;

    fn use_effect_state<T, E, F, D>(
        &mut self,
        init_f: F,
        deps: D,
    ) -> (State<Option<T>>, State<bool>, State<Option<E>>)
    where
        T: Send + Sync + Unpin,
        E: Send + Sync + Unpin,
        F: Future<Output = Result<T, E>> + Send + 'static,
        D: PartialEq + Unpin + 'static;
}

impl UseInitState for Hooks<'_, '_> {
    fn use_init_state<T, E, F>(
        &mut self,
        init_f: F,
    ) -> (State<Option<T>>, State<bool>, State<Option<E>>)
    where
        T: Send + Sync + Unpin,
        E: Send + Sync + Unpin,
        F: Future<Output = Result<T, E>> + Send + 'static,
    {
        let mut loading = self.use_state(|| false);
        let state = self.use_state(|| None::<T>);
        let error = self.use_state(|| None::<E>);

        self.use_future(async move {
            // 延迟200ms显示加载中，防止闪烁
            let notify = Arc::new(Notify::new());
            let join_handler = tokio::spawn({
                let notify = notify.clone();
                async move {
                    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                    if notify.notified().now_or_never().is_none() {
                        loading.set(true);
                    }
                }
            });

            match init_f.await {
                Ok(value) => {
                    state.write().replace(value);
                }
                Err(e) => {
                    error.write().replace(e);
                }
            }

            notify.notify_one();
            loading.set(false);
            let _ = join_handler.await;
        });

        (state, loading, error)
    }

    fn use_effect_state<T, E, F, D>(
        &mut self,
        init_f: F,
        deps: D,
    ) -> (State<Option<T>>, State<bool>, State<Option<E>>)
    where
        T: Send + Sync + Unpin,
        E: Send + Sync + Unpin,
        F: Future<Output = Result<T, E>> + Send + 'static,
        D: PartialEq + Unpin + 'static,
    {
        let mut loading = self.use_state(|| false);
        let state = self.use_state(|| None::<T>);
        let error = self.use_state(|| None::<E>);

        self.use_async_effect(
            async move {
                let notify = Arc::new(Notify::new());
                let join_handler = tokio::spawn({
                    let notify = notify.clone();
                    async move {
                        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                        if notify.notified().now_or_never().is_none() {
                            loading.set(true);
                        }
                    }
                });

                match init_f.await {
                    Ok(value) => {
                        state.write().replace(value);
                    }
                    Err(e) => {
                        error.write().replace(e);
                    }
                }

                notify.notify_one();
                loading.set(false);
                let _ = join_handler.await;
            },
            deps,
        );

        (state, loading, error)
    }
}
