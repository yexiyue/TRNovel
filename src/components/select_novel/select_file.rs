use crossterm::event::{KeyCode, KeyEventKind};
use ratatui::{
    style::{Style, Stylize},
    widgets::Scrollbar,
};
use std::path::PathBuf;
use tokio::sync::mpsc::UnboundedSender;
use tui_tree_widget::{Tree, TreeItem, TreeState};

use crate::{
    app::state::State,
    components::{Component, Info, KeyShortcutInfo},
    errors::Result,
    events::Events,
    routes::Route,
};

use super::empty::Empty;

#[derive(Debug, Default)]
pub struct SelectFile<'a> {
    pub state: TreeState<PathBuf>,
    pub items: Vec<TreeItem<'a, PathBuf>>,
}

impl<'a> SelectFile<'a> {
    pub fn new(items: Vec<TreeItem<'a, PathBuf>>) -> Result<Self> {
        Ok(Self {
            state: TreeState::default(),
            items,
        })
    }
}

impl Component for SelectFile<'_> {
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        if self.items.is_empty() {
            frame.render_widget(Empty::new("该目录下未搜索到小说文件"), area);
            return Ok(());
        }
        let tree_widget = Tree::new(&self.items)?
            .highlight_style(Style::new().bold().on_light_cyan())
            .experimental_scrollbar(Some(Scrollbar::default()));

        frame.render_stateful_widget(tree_widget, area, &mut self.state);
        Ok(())
    }

    fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        tx: UnboundedSender<Events>,
        _state: State,
    ) -> Result<()> {
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Char('h') | KeyCode::Left => {
                    self.state.key_left();
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.state.key_down();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.state.key_up();
                }
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                    let res = self.state.selected().last();
                    if let Some(path) = res {
                        if path.is_file() {
                            tx.send(Events::PushRoute(Route::ReadNovel(path.clone())))?;
                        } else {
                            self.state.toggle_selected();
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }
}

impl Info for SelectFile<'_> {
    fn key_shortcut_info(&self) -> KeyShortcutInfo {
        KeyShortcutInfo::new(vec![
            ("选择下一个", "J / ▼"),
            ("选择上一个", "K / ▲"),
            ("取消选择", "H / ◄"),
            ("确认选择", "L / ► / Enter"),
            ("切换到历史记录", "Tab"),
        ])
    }
}
