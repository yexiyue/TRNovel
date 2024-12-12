use crate::{app::State, Events, Result};
use crossterm::event::KeyEvent;
use ratatui::{layout::Rect, Frame};

pub mod loading_wrapper;
pub use loading_wrapper::*;
pub mod modal;
pub use modal::*;
pub mod empty;
pub use empty::*;

pub trait Component {
    /// 渲染组件样式
    fn render(&mut self, frame: &mut Frame, area: Rect) -> Result<()>;

    fn handle_key_event(&mut self, key: KeyEvent, _state: State) -> Result<Option<KeyEvent>> {
        Ok(Some(key))
    }

    /// 用于更新Loading tick
    fn handle_tick(&mut self, _state: State) -> Result<()> {
        Ok(())
    }

    /// 此架构是消耗型事件，优先级根据代码逻辑而定，推荐优先处理子组件
    /// 注意：一定要返回事件，不要返回None，因为render事件也是根据这个来的
    fn handle_events(&mut self, events: Events, state: State) -> Result<Option<Events>> {
        match events {
            Events::KeyEvent(key) => self
                .handle_key_event(key, state)
                .map(|item| item.map(Events::KeyEvent)),

            Events::Tick => {
                self.handle_tick(state)?;

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
