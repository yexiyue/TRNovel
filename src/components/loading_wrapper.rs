use std::time::Duration;

use crate::{
    app::State,
    components::Loading,
    errors::{Errors, Result},
    pages::{Page, PageWrapper},
    Events, Navigator, RoutePage, Router,
};
use anyhow::anyhow;
use async_trait::async_trait;
use tokio::{sync::mpsc, time::sleep};

use super::Component;

/// 一层抽象，在渲染loading的同时初始化，在返回None时，需要在init内调用navigator.replace
/// 避免back页面时看到的一直是loading
#[async_trait]
pub trait LoadingWrapperInit
where
    Self: Sized,
{
    type Arg;
    async fn init(args: Self::Arg, navigator: Navigator, state: State) -> Result<Option<Self>>;
}

pub enum LoadingWrapperMsg<T, A>
where
    T: Send + Sync,
    A: Send + Sync,
{
    Inner(T),
    InitArgs(A),
    Error(Errors),
}

/// 用于包装组件，需要进行耗时初始化时使用，比如扫描文件等。
pub struct LoadingWrapper<T, A>
where
    T: Send + Sync,
    A: Send + Sync,
{
    pub inner: Option<T>,
    pub loading: Loading,
    pub sender: tokio::sync::mpsc::Sender<LoadingWrapperMsg<T, A>>,
    pub state: State,
    pub navigator: Navigator,
}

unsafe impl<T, A> Send for LoadingWrapper<T, A>
where
    T: Send + Sync,
    A: Send + Sync,
{
}

#[async_trait]
impl<T, A> Page for LoadingWrapper<T, A>
where
    T: Send + Sync + LoadingWrapperInit<Arg = A> + 'static,
    A: Send + Sync + 'static,
{
    type Msg = LoadingWrapperMsg<T, A>;
    async fn update(&mut self, msg: Self::Msg) -> Result<()> {
        match msg {
            LoadingWrapperMsg::Inner(inner) => {
                self.inner = Some(inner);
            }
            LoadingWrapperMsg::InitArgs(args) => {
                let sender = self.sender.clone();
                let state = self.state.clone();
                let navigator = self.navigator.clone();
                tokio::spawn(async move {
                    if let Err(err) = (async {
                        if let Some(inner) = T::init(args, navigator, state).await? {
                            // 等待500毫秒防止闪屏
                            sleep(Duration::from_millis(500)).await;

                            sender
                                .send(LoadingWrapperMsg::Inner(inner))
                                .await
                                .map_err(|e| anyhow!(e))?;
                        }
                        Ok::<(), Errors>(())
                    })
                    .await
                    {
                        sender.send(LoadingWrapperMsg::Error(err)).await.unwrap();
                    }
                });
            }
            LoadingWrapperMsg::Error(err) => {
                return Err(err);
            }
        }
        Ok(())
    }
}

impl<T, A> LoadingWrapper<T, A>
where
    T: Send + Sync + 'static + LoadingWrapperInit<Arg = A> + Component,
    A: Send + Sync + 'static,
{
    /// 大部分new 是在事件中，或者路由首页，这些都可以直接拿到state参数和navigator参数
    pub fn new(
        sender: tokio::sync::mpsc::Sender<LoadingWrapperMsg<T, A>>,
        tip: &str,
        navigator: Navigator,
        state: State,
    ) -> Self {
        Self {
            inner: None,
            loading: Loading::new(tip),
            sender,
            state,
            navigator,
        }
    }

    pub fn init_args(&self, args: A) -> anyhow::Result<()> {
        self.sender.try_send(LoadingWrapperMsg::InitArgs(args))?;
        Ok(())
    }

    /// 快速创建路由页面
    pub fn route_page(
        tip: &str,
        navigator: Navigator,
        state: State,
        args: A,
    ) -> Result<Box<dyn RoutePage>> {
        let (tx, rx) = mpsc::channel(1);

        let loading_wrapper: Self = LoadingWrapper::new(tx, tip, navigator.clone(), state);
        loading_wrapper.init_args(args)?;

        Ok(Box::new(PageWrapper::new(loading_wrapper, Some(rx))))
    }
}

/// 不在router中init是因为会用到Option，比较麻烦
impl<T, A> Router for LoadingWrapper<T, A>
where
    T: Send + Sync + 'static + Component,
    A: Send + Sync + 'static,
{
}

impl<T, A> Component for LoadingWrapper<T, A>
where
    T: Send + Sync + 'static + Component,
    A: Send + Sync + 'static,
{
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        if let Some(inner) = &mut self.inner {
            inner.render(frame, area)?;
        } else {
            frame.render_widget(&mut self.loading, area);
        }
        Ok(())
    }

    fn key_shortcut_info(&self) -> crate::components::KeyShortcutInfo {
        if let Some(inner) = &self.inner {
            inner.key_shortcut_info()
        } else {
            Default::default()
        }
    }

    fn handle_events(&mut self, events: Events, state: State) -> Result<Option<Events>> {
        if let Some(inner) = &mut self.inner {
            inner.handle_events(events, state)
        } else {
            if matches!(events, Events::Tick) {
                self.handle_tick(state)?;
            }
            Ok(Some(events))
        }
    }

    fn handle_tick(&mut self, _state: State) -> Result<()> {
        self.loading.state.calc_next();
        Ok(())
    }
}
