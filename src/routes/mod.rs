use crate::{components::Component, events::Events};
use anyhow::Result;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};

mod route;
pub use route::Route;
use tokio::sync::mpsc::UnboundedSender;

pub struct Routes {
    pub current_router: usize,
    pub routes: Vec<Box<dyn Router>>,
    pub event_tx: UnboundedSender<Events>,
}

impl Routes {
    pub fn new(event_tx: UnboundedSender<Events>) -> Self {
        Self {
            current_router: 0,
            routes: Vec::new(),
            event_tx,
        }
    }

    pub fn push_router(&mut self,mut router: Box<dyn Router>) -> Result<()> {
        if self.current_router < self.routes.len().saturating_sub(1) {
            self.routes.drain(self.current_router + 1..);
        }
        router.as_mut().init(self.event_tx.clone())?;
        self.routes.push(router);
        self.current_router = self.routes.len().saturating_sub(1);

        Ok(())
    }

    pub fn replace_router(&mut self,mut router: Box<dyn Router>) -> Result<()> {
        router.as_mut().init(self.event_tx.clone())?;
        self.routes[self.current_router] = router;

        Ok(())
    }

    pub fn back(&mut self) {
        self.current_router = self.current_router.saturating_sub(1);
    }

    pub fn go(&mut self) {
        if self.current_router < self.routes.len() - 1 {
            self.current_router += 1;
        }
    }
}

pub trait Router: Component {
    fn init(&mut self, tx: UnboundedSender<Events>) -> Result<()>;
}

impl Component for Routes {
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        if self.routes.is_empty() {
            return Ok(());
        }
        let router = self.routes[self.current_router].as_mut();

        router.draw(frame, area)?;

        Ok(())
    }

    fn handle_events(&mut self, events: Events, tx: UnboundedSender<Events>) -> Result<()> {
        if !self.routes.is_empty() {
            let router = self.routes[self.current_router].as_mut();
            router.handle_events(events.clone(), tx.clone())?;
        }

        match events {
            Events::Back => self.back(),
            Events::Go => self.go(),
            Events::PushRoute(router) => self.push_router(router.to_page())?,
            Events::ReplaceRoute(router) => self.replace_router(router.to_page())?,
            Events::KeyEvent(key) => self.handle_key_event(key, tx)?,
            _ => {}
        };

        Ok(())
    }

    fn handle_key_event(&mut self, key: KeyEvent, tx: UnboundedSender<Events>) -> Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }
        match key.code {
            KeyCode::Char('b') => {
                tx.send(Events::Back)?;
            }
            KeyCode::Char('g') => {
                tx.send(Events::Go)?;
            }
            _ => {}
        }
        Ok(())
    }
}
