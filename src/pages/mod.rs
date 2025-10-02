use crate::{
    Events, Navigator, Result, RoutePage, Router,
    app::State,
    components::{Component, ShortcutInfo, ShortcutInfoState},
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use tokio::sync::mpsc;

pub mod local_novel;
pub mod network_novel;
pub mod read_novel;
pub use read_novel::ReadNovel;
pub mod home;
pub mod playground;
pub mod select_history;
pub mod theme_setting;

#[async_trait]
pub trait Page<Arg = ()>
where
    Self: Sized,
    Arg: Send + Sync + 'static,
{
    type Msg: Send + Sync;

    async fn init(
        arg: Arg,
        sender: mpsc::Sender<Self::Msg>,
        navigator: Navigator,
        state: State,
    ) -> Result<Self>;

    async fn update(&mut self, _msg: Self::Msg) -> Result<()> {
        Ok(())
    }
}

/// 实现自定义Msg，方便后面异步获取数据场景
/// [crate::components::LoadingWrapper] 就是典型例子
pub struct PageWrapper<T, A, Msg = ()>
where
    T: Page<A, Msg = Msg> + Router,
    A: Send + Sync + 'static,
{
    pub inner: Option<T>,
    pub msg_rx: mpsc::Receiver<Msg>,
    pub msg_tx: mpsc::Sender<Msg>,
    pub shortcut_info_state: ShortcutInfoState,
    pub arg: A,
}

impl<T, A, Msg> PageWrapper<T, A, Msg>
where
    T: Page<A, Msg = Msg> + Router,
    A: Send + Sync + 'static,
{
    pub fn new(arg: A, buffer: Option<usize>) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel(buffer.unwrap_or(1));

        Self {
            inner: None,
            msg_rx,
            msg_tx,
            shortcut_info_state: ShortcutInfoState::default(),
            arg,
        }
    }
}

#[async_trait]
impl<T, A, Msg> RoutePage for PageWrapper<T, A, Msg>
where
    T: Page<A, Msg = Msg> + Router,
    Msg: Send + Sync,
    A: Send + Sync + 'static + Clone,
{
    async fn init(&mut self, navigator: Navigator, state: State) -> Result<()> {
        let inner = T::init(self.arg.clone(), self.msg_tx.clone(), navigator, state).await?;
        self.inner = Some(inner);
        Ok(())
    }

    async fn update(&mut self) -> Result<()> {
        if let Ok(msg) = self.msg_rx.try_recv() {
            self.inner.as_mut().unwrap().update(msg).await?;
        }

        Ok(())
    }
}

#[async_trait]
impl<T, A, Msg> Router for PageWrapper<T, A, Msg>
where
    T: Page<A, Msg = Msg> + Router,
    Msg: Send + Sync,
    A: Send + Sync + 'static,
{
    async fn on_show(&mut self, state: State) -> crate::errors::Result<()> {
        self.inner.as_mut().unwrap().on_show(state).await
    }

    async fn on_hide(&mut self, state: State) -> crate::errors::Result<()> {
        self.inner.as_mut().unwrap().on_hide(state).await
    }

    async fn on_unmounted(&mut self, state: State) -> crate::errors::Result<()> {
        self.inner.as_mut().unwrap().on_unmounted(state).await
    }
}

#[async_trait]
impl<T, A, Msg> Component for PageWrapper<T, A, Msg>
where
    T: Page<A, Msg = Msg> + Router,
    Msg: Send + Sync,
    A: Send + Sync + 'static,
{
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        self.inner.as_mut().unwrap().render(frame, area)?;

        if self.shortcut_info_state.show {
            frame.render_stateful_widget(
                ShortcutInfo::new(self.inner.as_ref().unwrap().key_shortcut_info()),
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
        let events = if self.shortcut_info_state.show {
            Some(events)
        } else {
            self.inner
                .as_mut()
                .unwrap()
                .handle_events(events, state.clone())
                .await?
        };

        if let Some(events) = events {
            match events {
                Events::KeyEvent(key) => self
                    .handle_key_event(key, state)
                    .await
                    .map(|item| item.map(Events::KeyEvent)),
                other => Ok(Some(other)),
            }
        } else {
            Ok(None)
        }
    }
}
