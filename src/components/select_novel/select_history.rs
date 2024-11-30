use crossterm::event::{KeyCode, KeyEventKind};
use ratatui::{
    style::Stylize,
    text::{Line, Text},
    widgets::{Block, Padding, Paragraph},
};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc::UnboundedSender;
use tui_widget_list::{ListBuilder, ListState, ListView};

use super::empty::Empty;
use crate::{
    app::state::State,
    components::{Component, Confirm, ConfirmState},
    errors::Result,
    events::Events,
    history::History,
    routes::Route,
};

#[derive(Debug, Clone)]
pub struct SelectHistory {
    pub state: ListState,
    pub confirm_state: ConfirmState,
    pub history: Arc<Mutex<History>>,
}

impl SelectHistory {
    pub fn new(history: Arc<Mutex<History>>) -> Self {
        Self {
            history,
            state: ListState::default(),
            confirm_state: ConfirmState::default(),
        }
    }

    fn render_list(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let list_items = self.history.lock().unwrap().histories.clone();
        let length = list_items.len();
        let builder = ListBuilder::new(move |context| {
            let (_path, item) = &list_items[context.index];

            let block = if context.is_selected {
                Block::bordered()
                    .padding(Padding::horizontal(2))
                    .light_cyan()
            } else {
                Block::bordered().padding(Padding::horizontal(2))
            };

            let paragraph = Paragraph::new(Text::from(vec![
                Line::from(item.file_name.clone()),
                Line::from(item.current_chapter.clone()).centered(),
                Line::from(
                    format!(
                        "{}% {}",
                        item.percent,
                        item.last_read_at.format("%Y-%m-%d %H:%M:%S")
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

impl Component for SelectHistory {
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        if self.history.lock().unwrap().histories.is_empty() {
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

    fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        tx: UnboundedSender<Events>,
        _state: State,
    ) -> Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }
        if self.confirm_state.show {
            match key.code {
                KeyCode::Char('y') => {
                    self.confirm_state.confirm();
                }
                KeyCode::Char('n') => {
                    self.confirm_state.cancel();
                }
                KeyCode::Left | KeyCode::Right => {
                    self.confirm_state.toggle();
                }
                KeyCode::Esc | KeyCode::Tab => {
                    self.confirm_state.cancel();
                    self.confirm_state.hide();
                }
                KeyCode::Enter => {
                    if let Some(index) = self.state.selected {
                        if self.confirm_state.is_confirm() {
                            self.history.lock().unwrap().remove_index(index);
                            self.state.select(None);
                        }
                    }

                    self.confirm_state.hide();
                }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Char('h') | KeyCode::Left => {
                    self.state.select(None);
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    self.state.next();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.state.previous();
                }
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                    let Some(index) = self.state.selected else {
                        return Err("请选择历史记录".into());
                    };
                    let (path, _) = &self.history.lock().unwrap().histories[index];
                    tx.send(Events::PushRoute(Route::ReadNovel(path.clone())))?;
                }
                KeyCode::Char('d') => {
                    if self.state.selected.is_none() {
                        return Err("请选择历史记录".into());
                    }

                    self.confirm_state.show();
                }
                _ => {}
            }
        }
        Ok(())
    }
}
