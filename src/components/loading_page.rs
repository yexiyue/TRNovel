use crate::{
    app::state::State,
    components::{Component, Loading},
    errors::Result,
    events::Events,
};
use std::sync::Arc;
use tokio::sync::{mpsc::UnboundedSender, Mutex};

/// LoadingPage
/// 包装组件，用于在组件加载时显示loading
#[derive(Debug, Clone)]
pub struct LoadingPage<T, A>
where
    T: Sized + Component,
{
    pub inner: Arc<Mutex<Option<T>>>,
    pub loading: Loading,
    pub args: A,
}

impl<T, A> LoadingPage<T, A>
where
    T: Sized + Component,
{
    pub fn new(args: A, loading: Loading) -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            loading,
            args,
        }
    }
}

impl<T, A> Component for LoadingPage<T, A>
where
    T: Component,
{
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        if let Some(inner) = self.inner.try_lock()?.as_mut() {
            inner.draw(frame, area)?;
            return Ok(());
        }

        frame.render_widget(&mut self.loading, area);

        Ok(())
    }

    fn handle_events(
        &mut self,
        events: Events,
        tx: UnboundedSender<Events>,
        state: State,
    ) -> Result<()> {
        if let Some(inner) = self.inner.try_lock()?.as_mut() {
            inner.handle_events(events, tx, state)?;
            return Ok(());
        }
        if let Events::Tick = events {
            self.loading.state.calc_next();
        }

        Ok(())
    }
}
