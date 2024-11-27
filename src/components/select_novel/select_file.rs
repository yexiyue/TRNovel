use anyhow::Result;
use crossterm::event::{KeyCode, KeyEventKind};
use ratatui::{
    style::{Style, Stylize},
    widgets::Scrollbar,
};
use std::path::PathBuf;
use tui_tree_widget::{Tree, TreeItem, TreeState};

use crate::{actions::Actions, components::Component};

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

impl<'a> Component for SelectFile<'a> {
    fn draw(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> anyhow::Result<()> {
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

    fn handle_key_event(&mut self, key: crossterm::event::KeyEvent) -> Result<Option<Actions>> {
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Char('\n' | ' ') => {
                    self.state.toggle_selected();
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    self.state.key_left();
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    self.state.key_right();
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.state.key_down();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.state.key_up();
                }
                KeyCode::Esc => {
                    self.state.select(Vec::new());
                }
                KeyCode::Enter => {
                    let res = self.state.selected().last();
                    if let Some(res) = res {
                        if res.is_file() {
                            return Ok(Some(Actions::SelectedFile(res.clone())));
                        } else {
                            self.state.toggle_selected();
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(None)
    }
}
