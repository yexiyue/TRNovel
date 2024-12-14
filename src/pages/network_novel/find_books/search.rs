use anyhow::anyhow;
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    style::{Style, Stylize},
    widgets::Block,
};
use tokio::sync::mpsc;
use tui_textarea::{Input, Key, TextArea};

use crate::{app::State, components::Component};

use super::FindBooksMsg;

pub struct Search<'a> {
    pub textarea: TextArea<'a>,
    pub is_focus: bool,
    pub sender: mpsc::Sender<FindBooksMsg>,
}

impl Search<'_> {
    pub fn new(sender: mpsc::Sender<FindBooksMsg>) -> Self {
        let mut textarea = TextArea::default();
        textarea.set_cursor_line_style(Style::default());
        textarea.set_placeholder_text("请输入搜索内容");

        Self {
            sender,
            textarea,
            is_focus: false,
        }
    }
}

#[async_trait]
impl Component for Search<'_> {
    fn render(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> crate::Result<()> {
        if self.is_focus {
            self.textarea
                .set_block(Block::bordered().border_style(Style::default().light_green()));
            self.textarea
                .set_cursor_style(Style::default().on_dark_gray());
        } else {
            self.textarea
                .set_block(Block::bordered().border_style(Style::default().dim()));
            self.textarea.set_cursor_style(Style::default());
        }
        frame.render_widget(&self.textarea, area);
        Ok(())
    }

    async fn handle_key_event(
        &mut self,
        key: KeyEvent,
        _state: State,
    ) -> crate::Result<Option<KeyEvent>> {
        if self.is_focus {
            match Input::from(key) {
                Input { key: Key::Esc, .. } => {
                    self.is_focus = false;
                    Ok(None)
                }
                Input {
                    key: Key::Enter, ..
                } => {
                    self.is_focus = false;
                    self.sender
                        .send(FindBooksMsg::Search(self.textarea.lines()[0].to_string()))
                        .await
                        .map_err(|e| anyhow!(e))?;
                    Ok(None)
                }
                input => {
                    self.textarea.input(input);
                    Ok(None)
                }
            }
        } else if key.code == KeyCode::Char('s') && key.kind == KeyEventKind::Press {
            self.is_focus = true;
            Ok(None)
        } else {
            Ok(Some(key))
        }
    }
}
