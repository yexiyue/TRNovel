use crate::{
    app::State,
    book_source::BookSourceCache,
    components::{Component, Confirm, ConfirmState, Empty, KeyShortcutInfo},
    utils::time_to_string,
    Navigator, Result,
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    style::Stylize,
    text::{Line, Text},
    widgets::{Block, Padding, Paragraph},
};
use std::sync::{Arc, Mutex};
use tui_widget_list::{ListBuilder, ListState, ListView};
pub mod import;

#[derive(Debug, Clone)]
pub struct BookSourceManager {
    pub state: ListState,
    pub confirm_state: ConfirmState,
    pub book_sources: Arc<Mutex<BookSourceCache>>,
    pub navigator: Navigator,
}

impl BookSourceManager {
    pub fn new(history: Arc<Mutex<BookSourceCache>>, navigator: Navigator) -> Self {
        Self {
            book_sources: history,
            state: ListState::default(),
            confirm_state: ConfirmState::default(),
            navigator,
        }
    }

    fn render_list(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let list_items = self.book_sources.lock().unwrap().clone();
        let length = list_items.len();
        let builder = ListBuilder::new(move |context| {
            let item = &list_items[context.index];

            let block = if context.is_selected {
                Block::bordered()
                    .padding(Padding::horizontal(2))
                    .light_cyan()
            } else {
                Block::bordered().padding(Padding::horizontal(2))
            };

            let paragraph = Paragraph::new(Text::from(vec![
                Line::from(item.book_source_name.clone()).centered(),
                Line::from(
                    format!(
                        "{} {}",
                        item.book_source_url,
                        time_to_string(item.last_update_time).unwrap()
                    )
                    .dim(),
                )
                .right_aligned(),
            ]))
            .block(block);

            (paragraph, 5)
        });
        let widget = ListView::new(builder, length).infinite_scrolling(false);
        frame.render_stateful_widget(widget, area, &mut self.state);
    }
}

#[async_trait]
impl Component for BookSourceManager {
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        if self.book_sources.lock().unwrap().is_empty() {
            frame.render_widget(Empty::new("暂无历史记录"), area);
            return Ok(());
        }

        self.render_list(frame, area);

        frame.render_stateful_widget(
            Confirm::new("警告", "确认删除该历史记录吗？"),
            area,
            &mut self.confirm_state,
        );
        Ok(())
    }

    async fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        _state: State,
    ) -> Result<Option<KeyEvent>> {
        if key.kind != KeyEventKind::Press {
            return Ok(Some(key));
        }
        if self.confirm_state.show {
            match key.code {
                KeyCode::Char('y') => {
                    self.confirm_state.confirm();
                    Ok(None)
                }
                KeyCode::Left | KeyCode::Right | KeyCode::Char('h') | KeyCode::Char('l') => {
                    self.confirm_state.toggle();
                    Ok(None)
                }
                KeyCode::Enter => {
                    if let Some(index) = self.state.selected {
                        if self.confirm_state.is_confirm() {
                            self.book_sources.lock().unwrap().remove(index);
                            self.book_sources.lock().unwrap().save()?;
                            self.state.select(None);
                        }
                    }
                    self.confirm_state.hide();
                    Ok(None)
                }
                KeyCode::Char('n') => {
                    self.confirm_state.hide();
                    Ok(None)
                }
                _ => Ok(Some(key)),
            }
        } else {
            match key.code {
                KeyCode::Char('h') | KeyCode::Left => {
                    self.state.select(None);
                    Ok(None)
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.state.next();
                    Ok(None)
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.state.previous();
                    Ok(None)
                }
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                    let Some(index) = self.state.selected else {
                        return Err("请选择历史记录".into());
                    };
                    let item = &self.book_sources.lock().unwrap()[index];

                    Ok(None)
                }
                KeyCode::Char('d') => {
                    if self.state.selected.is_none() {
                        return Err("请选择历史记录".into());
                    }

                    self.confirm_state.show();
                    Ok(None)
                }
                _ => Ok(Some(key)),
            }
        }
    }

    fn key_shortcut_info(&self) -> crate::components::KeyShortcutInfo {
        let data = if self.confirm_state.show {
            vec![
                ("确认删除", "Y"),
                ("取消删除", "N"),
                ("切换确定/取消", "H / ◄ / L / ► "),
                ("确认选中", "Enter"),
                ("切换到选择文件", "Tab"),
            ]
        } else {
            vec![
                ("选择下一个", "J / ▼"),
                ("选择上一个", "K / ▲"),
                ("取消选择", "H / ◄"),
                ("确认选择", "L / ► / Enter"),
                ("删除选中的历史记录", "D"),
                ("切换到选择文件", "Tab"),
            ]
        };
        KeyShortcutInfo::new(data)
    }
}
