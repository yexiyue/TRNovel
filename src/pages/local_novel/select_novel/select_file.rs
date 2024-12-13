use crate::{
    app::State,
    components::{Component, Empty, KeyShortcutInfo, LoadingWrapper},
    pages::local_novel::ReadNovel,
    Navigator, Result,
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    style::{Style, Stylize},
    widgets::Scrollbar,
};
use std::path::PathBuf;
use tui_tree_widget::{Tree, TreeItem, TreeState};

#[derive(Debug)]
pub struct SelectFile<'a> {
    pub state: TreeState<PathBuf>,
    pub items: Vec<TreeItem<'a, PathBuf>>,
    pub navigator: Navigator,
}

impl<'a> SelectFile<'a> {
    pub fn new(items: Vec<TreeItem<'a, PathBuf>>, navigator: Navigator) -> Result<Self> {
        Ok(Self {
            state: TreeState::default(),
            items,
            navigator,
        })
    }
}

#[async_trait]
impl Component for SelectFile<'_> {
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
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

    async fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        state: State,
    ) -> Result<Option<KeyEvent>> {
        if key.kind == KeyEventKind::Press {
            match key.code {
                KeyCode::Char('h') | KeyCode::Left => {
                    self.state.key_left();
                    return Ok(None);
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.state.key_down();
                    return Ok(None);
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.state.key_up();
                    return Ok(None);
                }
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                    let res = self.state.selected().last();
                    if let Some(path) = res {
                        if path.is_file() {
                            self.navigator.push(
                                LoadingWrapper::<ReadNovel, PathBuf>::route_page(
                                    "加载小说中...",
                                    self.navigator.clone(),
                                    state,
                                    path.to_path_buf(),
                                )?,
                            )?;
                        } else {
                            self.state.toggle_selected();
                        }
                    }
                    return Ok(None);
                }
                _ => {
                    return Ok(Some(key));
                }
            }
        }
        Ok(Some(key))
    }

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
