pub mod components;
pub mod events;
pub mod pages;
pub mod router;
pub mod routes;
pub mod state;

use std::sync::{Arc, Mutex};

use components::Component;
use crossterm::event::{KeyCode, KeyEventKind, KeyModifiers};
pub use events::*;
pub use pages::*;
use ratatui::layout::Size;
use ratatui::{DefaultTerminal, Frame};
pub use router::*;
pub use routes::*;
// pub use state::*;

use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};
use tokio_util::sync::CancellationToken;

use crate::app::state::State;
use crate::components::Warning;
use crate::errors::{Errors, Result};
use crate::history::History;

pub struct App {
    pub show_exit: bool,
    pub event_rx: UnboundedReceiver<Events>,
    pub event_tx: UnboundedSender<Events>,
    pub error: Option<String>,
    pub warning: Option<String>,
    pub cancellation_token: CancellationToken,
    pub routes: Routes,
    pub state: State,
}

impl App {
    pub fn new() -> Result<Self> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        let cancellation_token = CancellationToken::new();

        event_loop(tx.clone(), cancellation_token.clone());

        let state = State {
            history: Arc::new(Mutex::new(History::load()?)),
            size: Arc::new(Mutex::new(None)),
        };

        Ok(Self {
            event_tx: tx.clone(),
            event_rx: rx,
            show_exit: false,
            error: None,
            warning: None,
            routes: Routes::new(vec![], 0, state.clone()),
            state,
            cancellation_token,
        })
    }

    pub fn exit(&mut self) {
        self.cancellation_token.cancel();
        self.show_exit = true;
    }

    pub fn render(&mut self, frame: &mut Frame<'_>) -> Result<()> {
        self.routes.render(frame, frame.area())?;

        if let Some(warning) = &self.error {
            frame.render_widget(Warning::new(warning, true), frame.area());
        }

        if let Some(warning) = &self.warning {
            frame.render_widget(Warning::new(warning, false), frame.area());
        }
        Ok(())
    }

    pub async fn handle_events(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        let Some(events) = self.event_rx.recv().await else {
            return Ok(());
        };

        let events = if self.error.is_none() && self.warning.is_none() {
            self.routes.handle_events(events, self.state.clone())?
        } else {
            Some(events)
        };

        if let Some(events) = events {
            match events {
                Events::KeyEvent(key) => {
                    if key.kind == KeyEventKind::Press {
                        if self.error.is_some() {
                            if key.code == KeyCode::Char('q') {
                                self.exit();
                            }
                        } else if self.warning.is_some() {
                            if key.code == KeyCode::Esc {
                                self.warning = None;
                            }
                        } else {
                            match key.code {
                                KeyCode::Char('q') => {
                                    self.exit();
                                }
                                KeyCode::Char('c') => {
                                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                                        self.exit();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
                }
                Events::Render => {
                    terminal.draw(|frame| match self.render(frame) {
                        Ok(_) => {}
                        Err(e) => {
                            self.error = Some(e.to_string());
                        }
                    })?;
                }
                Events::Error(e) => self.error = Some(e),
                Events::Resize(width, height) => {
                    self.state
                        .size
                        .lock()
                        .unwrap()
                        .replace(Size::new(width, height));
                }
                _ => {}
            }
        }
        Ok(())
    }

    pub async fn run(mut self, mut terminal: DefaultTerminal) -> Result<()> {
        let size = terminal.size()?;
        self.state.size.lock().unwrap().replace(size);

        while !self.show_exit {
            match self.handle_events(&mut terminal).await {
                Ok(_) => {}
                Err(e) => {
                    if let Errors::Warning(tip) = e {
                        self.warning = Some(tip);
                    } else {
                        self.error = Some(e.to_string());
                    }
                }
            }

            match self.routes.update().await {
                Ok(_) => {}
                Err(e) => {
                    if let Errors::Warning(tip) = e {
                        self.warning = Some(tip);
                    } else {
                        self.error = Some(e.to_string());
                    }
                }
            }
        }
        Ok(())
    }
}
