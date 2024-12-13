use crate::{
    app::State,
    components::{Component, ShortcutInfo, ShortcutInfoState},
    Events, Navigator, Result, RoutePage, Router,
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use tokio::sync::mpsc;

pub mod local_novel;
pub mod network_novel;

#[async_trait]
pub trait Page {
    type Msg: Send + Sync;
    async fn update(&mut self, _msg: Self::Msg) -> Result<()> {
        Ok(())
    }
}

/// 实现自定义Msg，方便后面异步获取数据场景
/// [crate::components::LoadingWrapper] 就是典型例子
pub struct PageWrapper<T, Msg = ()>
where
    T: Page<Msg = Msg> + Router,
{
    pub inner: T,
    pub msg_rx: Option<mpsc::Receiver<Msg>>,
    pub shortcut_info_state: ShortcutInfoState,
}

impl<T, Msg> PageWrapper<T, Msg>
where
    T: Page<Msg = Msg> + Router,
{
    pub fn new(inner: T, msg_rx: Option<mpsc::Receiver<Msg>>) -> Self {
        Self {
            inner,
            msg_rx,
            shortcut_info_state: ShortcutInfoState::default(),
        }
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

#[async_trait]
impl<T, Msg> Router for PageWrapper<T, Msg>
where
    T: Page<Msg = Msg> + Router,
    Msg: Send + Sync,
{
    async fn init(&mut self, navigator: Navigator, state: State) -> crate::errors::Result<()> {
        self.inner.init(navigator, state).await
    }

    async fn on_show(&mut self, state: State) -> crate::errors::Result<()> {
        self.inner.on_show(state).await
    }

    async fn on_hide(&mut self, state: State) -> crate::errors::Result<()> {
        self.inner.on_hide(state).await
    }

    async fn on_unmounted(&mut self, state: State) -> crate::errors::Result<()> {
        self.inner.on_unmounted(state).await
    }
}

#[async_trait]
impl<T, Msg> Component for PageWrapper<T, Msg>
where
    T: Page<Msg = Msg> + Router,
    Msg: Send + Sync,
{
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        self.inner.render(frame, area)?;

        if self.shortcut_info_state.show {
            frame.render_stateful_widget(
                ShortcutInfo::new(self.inner.key_shortcut_info()),
                area,
                &mut self.shortcut_info_state,
            );
        }
        Ok(())
    }

    async fn handle_key_event(&mut self, key: KeyEvent, _state: State) -> Result<Option<KeyEvent>> {
        if key.kind != KeyEventKind::Press {
            return Ok(Some(key));
        }
        if self.shortcut_info_state.show {
            match key.code {
                KeyCode::Char('i') => {
                    self.shortcut_info_state.toggle();
                    Ok(None)
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    if self.shortcut_info_state.show {
                        self.shortcut_info_state.scroll_down();
                    }
                    Ok(None)
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if self.shortcut_info_state.show {
                        self.shortcut_info_state.scroll_up();
                    }
                    Ok(None)
                }
                KeyCode::PageUp => {
                    if self.shortcut_info_state.show {
                        self.shortcut_info_state.scroll_page_up();
                    }
                    Ok(None)
                }
                KeyCode::PageDown => {
                    if self.shortcut_info_state.show {
                        self.shortcut_info_state.scroll_page_down();
                    }
                    Ok(None)
                }
                _ => Ok(Some(key)),
            }
        } else {
            match key.code {
                KeyCode::Char('i') => {
                    self.shortcut_info_state.toggle();
                    Ok(None)
                }
                _ => Ok(Some(key)),
            }
        }
    }

    async fn handle_events(&mut self, events: Events, state: State) -> Result<Option<Events>> {
        if let Some(events) = match events {
            Events::KeyEvent(key) => self
                .handle_key_event(key, state.clone())
                .await
                .map(|item| item.map(Events::KeyEvent)),
            _ => Ok(Some(events)),
        }? {
            if self.shortcut_info_state.show {
                Ok(Some(events))
            } else {
                self.inner.handle_events(events, state).await
            }
        } else {
            Ok(None)
        }
    }
}
