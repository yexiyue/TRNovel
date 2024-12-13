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

pub enum LoadingWrapperMsg<T>
where
    T: Send + Sync,
{
    Inner(T),
    Error(Errors),
}

/// 用于包装组件，需要进行耗时初始化时使用，比如扫描文件等。
pub struct LoadingWrapper<T>
where
    T: Send + Sync,
{
    pub inner: Option<T>,
    pub loading: Loading,
}

unsafe impl<T> Send for LoadingWrapper<T> where T: Send + Sync {}

impl<T, A> LoadingWrapper<T>
where
    T: Send + Sync + LoadingWrapperInit<Arg = A> + 'static + Router,
    A: Send + Sync + 'static + Clone,
{
    pub fn route_page(tip: &'static str, args: A, buffer: Option<usize>) -> Box<dyn RoutePage> {
        let res: PageWrapper<LoadingWrapper<T>, (&'static str, A), LoadingWrapperMsg<T>> =
            PageWrapper::new((tip, args), buffer);
        Box::new(res)
    }
}

#[async_trait]
impl<T, A> Page<(&'static str, A)> for LoadingWrapper<T>
where
    T: Send + Sync + LoadingWrapperInit<Arg = A> + 'static,
    A: Send + Sync + 'static,
{
    type Msg = LoadingWrapperMsg<T>;

    async fn init(
        arg: (&'static str, A),
        sender: mpsc::Sender<Self::Msg>,
        navigator: Navigator,
        state: State,
    ) -> Result<Self> {
        let (title, args) = arg;

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

        Ok(Self {
            inner: None,
            loading: Loading::new(title),
        })
    }

    async fn update(&mut self, msg: Self::Msg) -> Result<()> {
        match msg {
            LoadingWrapperMsg::Inner(inner) => {
                self.inner = Some(inner);
            }
            LoadingWrapperMsg::Error(err) => {
                return Err(err);
            }
        }
        Ok(())
    }
}

#[async_trait]
/// 不在router中init是因为会用到Option，比较麻烦
impl<T> Router for LoadingWrapper<T>
where
    T: Send + Sync + 'static + Component + Router,
{
    async fn on_show(&mut self, state: State) -> Result<()> {
        if let Some(inner) = &mut self.inner {
            inner.on_show(state).await?;
        }
        Ok(())
    }

    async fn on_hide(&mut self, state: State) -> Result<()> {
        if let Some(inner) = &mut self.inner {
            inner.on_hide(state).await?;
        }
        Ok(())
    }

    async fn on_unmounted(&mut self, state: State) -> Result<()> {
        self.on_hide(state.clone()).await?;
        if let Some(inner) = &mut self.inner {
            inner.on_unmounted(state).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl<T> Component for LoadingWrapper<T>
where
    T: Send + Sync + 'static + Component,
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

    async fn handle_events(&mut self, events: Events, state: State) -> Result<Option<Events>> {
        if let Some(inner) = &mut self.inner {
            inner.handle_events(events, state).await
        } else {
            if matches!(events, Events::Tick) {
                self.handle_tick(state).await?;
            }
            Ok(Some(events))
        }
    }

    async fn handle_tick(&mut self, _state: State) -> Result<()> {
        self.loading.state.calc_next();
        Ok(())
    }
}
