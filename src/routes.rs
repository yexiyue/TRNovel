use crate::{app::State, components::Component, Events, Result, RoutePage, RouterMsg};
use anyhow::anyhow;
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEventKind};
use tokio::sync::mpsc::{Receiver, Sender};

/// 路由
/// 模式跟page模式很像，但为了防止嵌套路由，所以就单独实现消息处理
pub struct Routes {
    pub routes: Vec<Box<dyn RoutePage>>,
    pub tx: Sender<RouterMsg>,
    pub rx: Receiver<RouterMsg>,
    pub current_router: usize,
    pub state: State,
}

impl Routes {
    pub async fn push_router(&mut self, mut router: Box<dyn RoutePage>) -> Result<()> {
        let state = self.state.clone();
        self.current()?.on_hide(state.clone()).await?;

        if self.current_router < self.routes.len().saturating_sub(1) {
            self.routes.drain(self.current_router + 1..);
        }
        router
            .as_mut()
            .init((&self.tx).into(), state.clone())
            .await?;

        self.routes.push(router);
        self.current_router = self.routes.len().saturating_sub(1);
        self.current()?.on_show(state).await?;

        Ok(())
    }

    pub async fn replace_router(&mut self, mut router: Box<dyn RoutePage>) -> Result<()> {
        let state = self.state.clone();
        self.current()?.on_hide(state.clone()).await?;
        router
            .as_mut()
            .init((&self.tx).into(), self.state.clone())
            .await?;

        self.routes[self.current_router] = router;

        self.current()?.on_show(state).await?;

        Ok(())
    }

    pub async fn back(&mut self) -> Result<()> {
        let state = self.state.clone();
        self.current()?.on_hide(state.clone()).await?;

        self.current_router = self.current_router.saturating_sub(1);

        self.current()?.on_show(state).await?;
        Ok(())
    }

    pub async fn pop(&mut self) -> Result<()> {
        let state = self.state.clone();
        self.current()?.on_hide(state.clone()).await?;
        self.current()?.on_unmounted(state.clone()).await?;

        if self.routes.len() > 1 {
            self.routes.pop();
            self.current_router = self.routes.len().saturating_sub(1);
            self.current()?.on_show(state).await?;
        }
        Ok(())
    }

    pub async fn go(&mut self) -> Result<()> {
        let state = self.state.clone();
        self.current()?.on_hide(state.clone()).await?;

        if self.current_router < self.routes.len() - 1 {
            self.current_router += 1;
            self.current()?.on_show(state).await?;
        }

        Ok(())
    }

    pub fn current(&mut self) -> Result<&mut Box<dyn RoutePage>> {
        self.routes
            .get_mut(self.current_router)
            .ok_or(anyhow!("No current route").into())
    }

    pub async fn update(&mut self) -> Result<()> {
        if let Ok(msg) = self.rx.try_recv() {
            match msg {
                RouterMsg::Back => self.back().await?,
                RouterMsg::Pop => self.pop().await?,
                RouterMsg::Go => self.go().await?,
                RouterMsg::ReplaceRoute(router) => self.replace_router(router).await?,
                RouterMsg::PushRoute(router) => self.push_router(router).await?,
            }
        }

        self.current()?.update().await?;

        Ok(())
    }
}

#[async_trait]
impl Component for Routes {
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        self.current()?.render(frame, area)?;
        Ok(())
    }

    async fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        _state: State,
    ) -> Result<Option<crossterm::event::KeyEvent>> {
        if key.kind != KeyEventKind::Press {
            return Ok(Some(key));
        }
        match key.code {
            KeyCode::Char('b') => {
                self.back().await?;
                Ok(None)
            }
            KeyCode::Char('g') => {
                self.go().await?;
                Ok(None)
            }
            KeyCode::Backspace => {
                self.pop().await?;
                Ok(None)
            }
            _ => Ok(Some(key)),
        }
    }

    async fn handle_events(&mut self, events: Events, state: State) -> Result<Option<Events>> {
        let Some(events) = self.current()?.handle_events(events, state.clone()).await? else {
            return Ok(None);
        };

        match events {
            Events::KeyEvent(key) => self
                .handle_key_event(key, state)
                .await
                .map(|item| item.map(Events::KeyEvent)),
            _ => Ok(Some(events)),
        }
    }
}
