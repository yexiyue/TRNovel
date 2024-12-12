use crate::app::state::State;
use crate::errors::Result;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

use super::Events;

pub trait Component {
    fn render(&mut self, frame: &mut Frame, area: Rect) -> Result<()>;

    fn handle_key_event(&mut self, key: KeyEvent, _state: State) -> Result<Option<KeyEvent>> {
        Ok(Some(key))
    }

    fn handle_tick(&mut self, _state: State) -> Result<()> {
        Ok(())
    }

    fn handle_events(&mut self, events: Events, state: State) -> Result<Option<Events>> {
        match events {
            Events::KeyEvent(key) => self
                .handle_key_event(key, state)
                .map(|item| item.map(Events::KeyEvent)),

            Events::Tick => {
                self.handle_tick(state)?;

                Ok(Some(Events::Tick))
            }
            _ => Ok(Some(events)),
        }
    }
}
