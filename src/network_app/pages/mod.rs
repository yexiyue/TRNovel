use super::{components::Component, RoutePage, Router};
use crate::errors::Result;
use async_trait::async_trait;
use tokio::sync::mpsc;
pub mod network_novel;
pub use network_novel::*;

#[async_trait]
pub trait Page {
    type Msg: Send + Sync;
    async fn update(&mut self, _msg: Self::Msg) -> Result<()> {
        Ok(())
    }
}

pub struct PageWrapper<T, Msg = ()>
where
    T: Page<Msg = Msg> + Router,
{
    pub inner: T,
    pub msg_rx: Option<mpsc::Receiver<Msg>>,
}

impl<T, Msg> PageWrapper<T, Msg>
where
    T: Page<Msg = Msg> + Router,
{
    pub fn new(inner: T, msg_rx: Option<mpsc::Receiver<Msg>>) -> Self {
        Self { inner, msg_rx }
    }
}

#[async_trait]
impl<T, Msg> RoutePage for PageWrapper<T, Msg>
where
    T: Page<Msg = Msg> + Router,
    Msg: Send + Sync,
{
    async fn update(&mut self) -> Result<()> {
        if let Some(rx) = self.msg_rx.as_mut() {
            if let Ok(msg) = rx.try_recv() {
                self.inner.update(msg).await?;
            }
        }

        Ok(())
    }
}

impl<T, Msg> Router for PageWrapper<T, Msg>
where
    T: Page<Msg = Msg> + Router,
    Msg: Send + Sync,
{
    fn init(&mut self, navigator: super::Navigator) -> crate::errors::Result<()> {
        self.inner.init(navigator)
    }
}

impl<T, Msg> Component for PageWrapper<T, Msg>
where
    T: Page<Msg = Msg> + Router,
    Msg: Send + Sync,
{
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        self.inner.render(frame, area)
    }
}
