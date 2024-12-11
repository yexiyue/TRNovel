use crate::errors::Result;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};
use tokio::sync::mpsc::UnboundedSender;
mod page;
use crate::{app::state::State, events::Events};
pub use page::Page;

pub mod modal;
pub mod network_novel;
pub mod read_novel;
pub mod select_novel;

pub use modal::*;
pub use network_novel::*;
pub use read_novel::*;
pub use select_novel::*;

pub trait Component {
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()>;

    fn handle_key_event(
        &mut self,
        key: KeyEvent,
        _tx: UnboundedSender<Events>,
        _state: State,
    ) -> Result<()> {
        let _ = key;
        Ok(())
    }

    fn handle_events(
        &mut self,
        events: Events,
        tx: UnboundedSender<Events>,
        state: State,
    ) -> Result<()> {
        if let Events::KeyEvent(event) = events {
            self.handle_key_event(event, tx, state)?;
        }

        Ok(())
    }
}

pub trait Info {
    fn key_shortcut_info(&self) -> KeyShortcutInfo;
}
