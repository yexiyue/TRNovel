use anyhow::anyhow;
use crossterm::event::{KeyCode, KeyEventKind};
use ratatui::{
    style::Stylize,
    text::{Line, Text},
    widgets::{Block, Padding, Paragraph},
};
use tokio::sync::mpsc::UnboundedSender;
use tui_widget_list::{ListBuilder, ListState, ListView};

use super::empty::Empty;
use crate::{components::Component, events::Events, history::History, routes::Route};

#[derive(Debug, Clone)]
pub struct SelectHistory {
    pub state: ListState,
    pub history: History,
}

impl SelectHistory {
    pub fn new(history: History) -> Self {
        Self {
            state: ListState::default(),
            history,
        }
    }

    fn render_list(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let list_items = self.history.histories.clone();
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
        let widget = ListView::new(builder, self.history.histories.len()).infinite_scrolling(false);
        frame.render_stateful_widget(widget, area, &mut self.state);
    }
}

impl Component for SelectHistory {
    fn draw(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> anyhow::Result<()> {
        if self.history.histories.is_empty() {
            frame.render_widget(Empty::new("暂无历史记录"), area);
            return Ok(());
        }
        self.render_list(frame, area);
        Ok(())
    }

    fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        tx: UnboundedSender<Events>,
    ) -> anyhow::Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }
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
                    return Err(anyhow!("No selected item"));
                };
                let (path, _) = &self.history.histories[index];
                tx.send(Events::PushRoute(Route::ReadNovel(path.clone())))?;
            }
            _ => {}
        }
        Ok(())
    }
}
