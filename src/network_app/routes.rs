use super::{components::Component, Events, RoutePage, RouterMsg};
use crate::{app::state::State, errors::Result};
use anyhow::anyhow;
use crossterm::event::{KeyCode, KeyEventKind};
use tokio::sync::mpsc::{Receiver, Sender};

pub struct Routes {
    pub routes: Vec<Box<dyn RoutePage>>,
    pub tx: Sender<RouterMsg>,
    pub rx: Receiver<RouterMsg>,
    pub current_router: usize,
    pub state: State,
}

impl Routes {
    pub fn new(routes: Vec<Box<dyn RoutePage>>, current_router: usize, state: State) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        Self {
            current_router,
            routes,
            state,
            tx,
            rx,
        }
    }
    pub fn push_router(&mut self, mut router: Box<dyn RoutePage>) -> Result<()> {
        if self.current_router < self.routes.len().saturating_sub(1) {
            self.routes.drain(self.current_router + 1..);
        }
        router.as_mut().init((&self.tx).into())?;

        self.routes.push(router);
        self.current_router = self.routes.len().saturating_sub(1);

        Ok(())
    }

    pub fn replace_router(&mut self, mut router: Box<dyn RoutePage>) -> Result<()> {
        router.as_mut().init((&self.tx).into())?;

        self.routes[self.current_router] = router;

        Ok(())
    }

    pub fn back(&mut self) {
        self.current_router = self.current_router.saturating_sub(1);
    }

    pub fn pop(&mut self) {
        if self.routes.len() > 1 {
            self.routes.pop();
            self.current_router = self.routes.len().saturating_sub(1);
        }
    }

    pub fn go(&mut self) {
        if self.current_router < self.routes.len() - 1 {
            self.current_router += 1;
        }
    }

    pub fn current(&mut self) -> Result<&mut Box<dyn RoutePage>> {
        self.routes
            .get_mut(self.current_router)
            .ok_or(anyhow!("No current route").into())
    }

    pub async fn update(&mut self) -> Result<()> {
        if let Ok(msg) = self.rx.try_recv() {
            match msg {
                RouterMsg::Back => self.back(),
                RouterMsg::Pop => self.pop(),
                RouterMsg::Go => self.go(),
                RouterMsg::ReplaceRoute(router) => self.replace_router(router)?,
                RouterMsg::PushRoute(router) => self.push_router(router)?,
            }
        }

        self.current()?.update().await?;

        Ok(())
    }
}

impl Component for Routes {
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        self.current()?.render(frame, area)?;
        Ok(())
    }

    fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        _state: State,
    ) -> Result<Option<crossterm::event::KeyEvent>> {
        if key.kind != KeyEventKind::Press {
            return Ok(Some(key));
        }
        match key.code {
            KeyCode::Char('b') => {
                self.back();
                Ok(None)
            }
            KeyCode::Char('g') => {
                self.go();
                Ok(None)
            }
            KeyCode::Backspace => {
                self.pop();
                Ok(None)
            }
            _ => Ok(Some(key)),
        }
    }

    fn handle_events(&mut self, events: Events, state: State) -> Result<Option<Events>> {
        let Some(events) = self.current()?.handle_events(events, state.clone())? else {
            return Ok(None);
        };

        match events {
            Events::KeyEvent(key) => self
                .handle_key_event(key, state)
                .map(|item| item.map(Events::KeyEvent)),
            _ => Ok(Some(events)),
        }
    }
}
