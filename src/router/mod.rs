use super::components::Component;
use crate::{app::State, Result};
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
    /// 初始化，传递路由和全局状态
    /// 这里为什么是方法而不是函数，因为在 [crate::routes::Routes] 中用的是Trait object，如果用函数就得知道具体的类型了
    /// 例如：[crate::components::LoadingWrapper] 的 `route_page`方法，
    /// ```rust
    /// LoadingWrapper::<SelectNovel, PathBuf>::route_page(
    ///    "扫描文件中...",
    ///    navigator.clone(),
    ///    state,
    ///    path,
    ///)
    /// ```
    fn init(&mut self, navigator: Navigator, state: State) -> Result<()> {
        let _ = navigator;
        let _ = state;
        Ok(())
    }
}

#[async_trait]
pub trait RoutePage: Router {
    async fn update(&mut self) -> Result<()>;
}
