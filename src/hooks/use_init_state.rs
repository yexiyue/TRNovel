use ratatui_kit::{Hooks, State, UseEffect, UseFuture, UseState};

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
        let mut loading = self.use_state(|| true);
        let state = self.use_state(|| None::<T>);
        let error = self.use_state(|| None::<E>);

        self.use_future(async move {
            loading.set(true);
            match init_f.await {
                Ok(value) => {
                    state.write().replace(value);
                }
                Err(e) => {
                    error.write().replace(e);
                }
            }
            loading.set(false);
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
        let mut loading = self.use_state(|| true);
        let state = self.use_state(|| None::<T>);
        let error = self.use_state(|| None::<E>);

        self.use_async_effect(
            async move {
                loading.set(true);
                match init_f.await {
                    Ok(value) => {
                        state.write().replace(value);
                    }
                    Err(e) => {
                        error.write().replace(e);
                    }
                }
                loading.set(false);
            },
            deps,
        );

        (state, loading, error)
    }
}
