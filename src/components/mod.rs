use crate::{app::State, Events, Result};
use async_trait::async_trait;
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

pub mod loading_wrapper;
pub use loading_wrapper::*;
pub mod modal;
pub use modal::*;
pub mod empty;
pub use empty::*;

#[async_trait]
pub trait Component {
    /// 渲染组件样式
    fn render(&mut self, frame: &mut Frame, area: Rect) -> Result<()>;

    async fn handle_key_event(&mut self, key: KeyEvent, _state: State) -> Result<Option<KeyEvent>> {
        Ok(Some(key))
    }

    /// 用于更新Loading tick
    async fn handle_tick(&mut self, _state: State) -> Result<()> {
        Ok(())
    }

    /// 此架构是消耗型事件，优先级根据代码逻辑而定，推荐优先处理子组件
    /// 注意：
    /// 1. 一定要返回事件，不要返回None，因为render事件也是根据这个来的
    /// 2. 不要执行特别耗时的任务，否则会影响到渲染，如果使用耗时任务请使用异步消息
    async fn handle_events(&mut self, events: Events, state: State) -> Result<Option<Events>> {
        match events {
            Events::KeyEvent(key) => self
                .handle_key_event(key, state)
                .await
                .map(|item| item.map(Events::KeyEvent)),

            Events::Tick => {
                self.handle_tick(state).await?;

                Ok(Some(Events::Tick))
            }
            _ => Ok(Some(events)),
        }
    }

    /// 快捷键提醒
    fn key_shortcut_info(&self) -> KeyShortcutInfo {
        KeyShortcutInfo::default()
    }
}
