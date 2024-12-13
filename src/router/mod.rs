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

#[async_trait]
pub trait Router: Send + Sync + Component {
    /// 路由初始化，传递路由和全局状态
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
    async fn init(&mut self, navigator: Navigator, state: State) -> Result<()> {
        let _ = navigator;
        let _ = state;
        Ok(())
    }

    /// 路由切换到后台时调用
    async fn on_hide(&mut self, state: State) -> Result<()> {
        let _ = state;
        Ok(())
    }

    /// 路由切换到前台时调用
    async fn on_show(&mut self, state: State) -> Result<()> {
        let _ = state;
        Ok(())
    }

    /// 路由被卸载时调用
    /// 这里会先调用 `on_hide` 方法，再调用 `on_unmounted` 方法
    async fn on_unmounted(&mut self, state: State) -> Result<()> {
        let _ = state;
        Ok(())
    }
}

/// 路由页面，这里将update方法抽离出来
/// 在[crate::pages::PageWrapper]中，实现异步消息的处理
#[async_trait]
pub trait RoutePage: Router {
    async fn update(&mut self) -> Result<()>;
}
