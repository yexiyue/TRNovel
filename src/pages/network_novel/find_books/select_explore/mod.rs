mod state;
mod widget;
use anyhow::anyhow;
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use parse_book_source::Explores;
use ratatui::widgets::StatefulWidgetRef;
use state::SelectExploreState;
use tokio::sync::mpsc;
use widget::SelectExploreWidget;

use crate::{app::State, components::Component};

use super::FindBooksMsg;

pub struct SelectExplore<'a> {
    pub widget: SelectExploreWidget<'a>,
    pub state: SelectExploreState,
    pub sender: mpsc::Sender<FindBooksMsg>,
}

impl SelectExplore<'_> {
    pub fn new(explore: Explores, sender: mpsc::Sender<FindBooksMsg>) -> Self {
        Self {
            widget: SelectExploreWidget::new(explore),
            state: SelectExploreState::default(),
            sender,
        }
    }
}

#[async_trait]
impl Component for SelectExplore<'_> {
    fn render(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> crate::Result<()> {
        self.widget
            .render_ref(area, frame.buffer_mut(), &mut self.state);
        Ok(())
    }

    async fn handle_key_event(
        &mut self,
        key: KeyEvent,
        _state: State,
    ) -> crate::Result<Option<KeyEvent>> {
        if key.kind != KeyEventKind::Press {
            return Ok(Some(key));
        }
        if self.state.show {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.state.select_next();
                    Ok(None)
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.state.select_previous();
                    Ok(None)
                }
                KeyCode::Enter => {
                    if let Some(index) = self.state.selected() {
                        let explore_item = self
                            .widget
                            .explore
                            .get(index)
                            .ok_or(anyhow!("Not found explore item"))?;

                        self.sender
                            .send(FindBooksMsg::SelectExplore(explore_item.clone()))
                            .await
                            .map_err(|e| anyhow!("Send message error: {}", e))?;
                    }
                    self.state.toggle();
                    Ok(None)
                }
                KeyCode::Tab | KeyCode::Esc => {
                    self.state.toggle();
                    Ok(None)
                }
                _ => Ok(Some(key)),
            }
        } else {
            match key.code {
                KeyCode::Tab => {
                    self.state.toggle();
                    Ok(None)
                }
                _ => Ok(Some(key)),
            }
        }
    }

    fn key_shortcut_info(&self) -> crate::components::KeyShortcutInfo {
        todo!()
    }
}
