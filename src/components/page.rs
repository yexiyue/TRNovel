use crate::{
    app::state::State,
    components::{Component, Loading},
    errors::Result,
    events::Events,
};
use crossterm::event::{KeyCode, KeyEventKind};
use std::sync::Arc;
use tokio::sync::{mpsc::UnboundedSender, Mutex};

use super::{Info, ShortcutInfo, ShortcutInfoState};

/// Page
/// 包装组件，用于在组件加载时显示loading 和提示案件信息
#[derive(Debug, Clone)]
pub struct Page<T, A>
where
    T: Sized + Component + Info,
{
    pub inner: Arc<Mutex<Option<T>>>,
    pub loading: Loading,
    pub shortcut_info: ShortcutInfoState,
    pub args: A,
}

impl<T, A> Page<T, A>
where
    T: Sized + Component + Info,
{
    pub fn new(args: A, loading: Loading) -> Self {
        Self {
            inner: Arc::new(Mutex::new(None)),
            shortcut_info: ShortcutInfoState::default(),
            loading,
            args,
        }
    }
}

impl<T, A> Component for Page<T, A>
where
    T: Component + Info,
{
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        if let Some(inner) = self.inner.try_lock()?.as_mut() {
            inner.draw(frame, area)?;
            if self.shortcut_info.show {
                frame.render_stateful_widget(
                    ShortcutInfo::new(inner.key_shortcut_info()),
                    area,
                    &mut self.shortcut_info,
                );
            }

            return Ok(());
        }

        frame.render_widget(&mut self.loading, area);

        Ok(())
    }

    fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        _tx: UnboundedSender<Events>,
        _state: State,
    ) -> Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }
        match key.code {
            KeyCode::Char('i') => {
                self.shortcut_info.toggle();
            }
            KeyCode::Char('j') | KeyCode::Down => {
                if self.shortcut_info.show {
                    self.shortcut_info.scroll_down();
                }
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.shortcut_info.show {
                    self.shortcut_info.scroll_up();
                }
            }
            KeyCode::PageUp => {
                if self.shortcut_info.show {
                    self.shortcut_info.scroll_page_up();
                }
            }
            KeyCode::PageDown => {
                if self.shortcut_info.show {
                    self.shortcut_info.scroll_page_down();
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_events(
        &mut self,
        events: Events,
        tx: UnboundedSender<Events>,
        state: State,
    ) -> Result<()> {
        if let Events::KeyEvent(key) = events {
            self.handle_key_event(key, tx.clone(), state.clone())?;
        }

        if self.shortcut_info.show {
            return Ok(());
        }

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
