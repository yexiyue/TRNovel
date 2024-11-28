use tokio::sync::mpsc::UnboundedSender;

use crate::{
    components::{loading::Loading, Component},
    events::Events,
};

pub struct LoadingPage<T, A>
where
    T: Sized + Component,
{
    pub inner: Option<T>,
    pub loading: Option<Loading>,
    pub args: A,
}

impl<T, A> LoadingPage<T, A>
where
    T: Sized + Component,
{
    pub fn new(args: A, loading: Option<Loading>) -> Self {
        Self {
            inner: None,
            loading,
            args,
        }
    }
}

impl<T, A> Component for LoadingPage<T, A>
where
    T: Component,
{
    fn draw(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> anyhow::Result<()> {
        if let Some(inner) = self.inner.as_mut() {
            inner.draw(frame, area)?;
            return Ok(());
        }
        if let Some(loading) = &mut self.loading {
            frame.render_widget(loading, area);
        }
        Ok(())
    }

    fn handle_events(&mut self, events: Events, tx: UnboundedSender<Events>) -> anyhow::Result<()> {
        if let Some(inner) = &mut self.inner {
            inner.handle_events(events, tx)?;
            return Ok(());
        }
        if let Some(loading) = &mut self.loading {
            match events {
                Events::Tick => {
                    loading.state.calc_next();
                }
                _ => {}
            }
        }

        Ok(())
    }
}
