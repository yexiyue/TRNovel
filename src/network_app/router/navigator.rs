use super::{RoutePage, RouterMsg};
use anyhow::Result;
use tokio::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub struct Navigator {
    pub tx: Sender<RouterMsg>,
}

impl From<&Sender<RouterMsg>> for Navigator {
    fn from(tx: &Sender<RouterMsg>) -> Self {
        Self::new(tx.clone())
    }
}

impl Navigator {
    pub fn new(tx: Sender<RouterMsg>) -> Self {
        Self { tx }
    }

    pub fn go(&self) -> Result<()> {
        Ok(self.tx.try_send(RouterMsg::Go)?)
    }

    pub fn back(&self) -> Result<()> {
        Ok(self.tx.try_send(RouterMsg::Back)?)
    }
    pub fn pop(&self) -> Result<()> {
        Ok(self.tx.try_send(RouterMsg::Pop)?)
    }

    pub fn push(&self, route: Box<dyn RoutePage>) -> Result<()> {
        Ok(self.tx.try_send(RouterMsg::PushRoute(route))?)
    }

    pub fn replace(&self, route: Box<dyn RoutePage>) -> Result<()> {
        Ok(self.tx.try_send(RouterMsg::ReplaceRoute(route))?)
    }
}
