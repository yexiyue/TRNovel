use anyhow::Result;
use crossterm::event::{Event, KeyEvent, MouseEvent};
use ratatui::{layout::Rect, Frame};

use crate::{actions::Actions, events::Events};

pub mod loading;
pub mod read_novel;
pub mod select_novel;
pub mod warning;

pub trait Component {
    fn draw(&mut self, frame: &mut Frame, area: Rect) -> Result<()>;

    fn handle_key_event(&mut self, key: KeyEvent) -> Result<Option<Actions>> {
        let _ = key;
        Ok(None)
    }

    fn handle_mouse_event(&mut self, key: MouseEvent) -> Result<Option<Actions>> {
        let _ = key;
        Ok(None)
    }

    fn handle_term_events(&mut self, event: Event) -> Result<Option<Actions>> {
        match event {
            Event::Key(key) => self.handle_key_event(key),
            Event::Mouse(key) => self.handle_mouse_event(key),
            _ => Ok(None),
        }
    }

    fn handle_events(
        &mut self,
        events: Events,
        tx: tokio::sync::mpsc::UnboundedSender<Events>,
    ) -> Result<()> {
        let _ = tx;
        match events {
            Events::CrosstermEvent(event) => {
                self.handle_term_events(event)?;
            }
            _ => {}
        }
        Ok(())
    }
}
