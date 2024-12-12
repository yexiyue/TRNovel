use super::components::Component;
use crate::errors::Result;
use async_trait::async_trait;

pub mod navigator;
pub use navigator::Navigator;

pub enum RouterMsg {
    Go,
    Back,
    Pop,
    PushRoute(Box<dyn RoutePage>),
    ReplaceRoute(Box<dyn RoutePage>),
}

pub trait Router: Send + Sync + Component {
    fn init(&mut self, navigator: Navigator) -> Result<()> {
        let _ = navigator;
        Ok(())
    }
}

#[async_trait]
pub trait RoutePage: Router {
    async fn update(&mut self) -> Result<()>;
}
