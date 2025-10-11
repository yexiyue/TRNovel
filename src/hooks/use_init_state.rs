use futures::FutureExt;
use ratatui_kit::{Hooks, State, UseEffect, UseFuture, UseState};
use std::sync::Arc;
use tokio::sync::Notify;

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
        D: std::hash::Hash;
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
                    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
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
        D: std::hash::Hash,
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
                        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
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
