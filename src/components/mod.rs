use anyhow::Result;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};
use tokio::sync::mpsc::UnboundedSender;
mod loading_page;
use crate::events::Events;
pub use loading_page::LoadingPage;

pub mod loading;
pub mod read_novel;
pub mod select_novel;
pub mod warning;

pub trait Component {
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()>;

    fn handle_key_event(&mut self, key: KeyEvent, _tx: UnboundedSender<Events>) -> Result<()> {
        let _ = key;
        Ok(())
    }

    fn handle_events(&mut self, events: Events, tx: UnboundedSender<Events>) -> Result<()> {
        match events {
            Events::KeyEvent(event) => {
                self.handle_key_event(event, tx)?;
            }
            _ => {}
        }
        Ok(())
    }
}
