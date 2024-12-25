use super::ReadNovelMsg;
use crate::{
    app::State,
    components::{Component, Empty, Search},
    novel::Novel,
    Result,
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, List, ListState, Padding, Scrollbar, ScrollbarState},
};
use tokio::sync::mpsc;

pub struct SelectChapter<'a, T>
where
    T: Novel + Send + Sync + 'static,
{
    pub state: ListState,
    pub list: List<'a>,
    pub search: Search<'a>,
    pub sender: mpsc::Sender<ReadNovelMsg<T>>,
    pub scrollbar_state: ScrollbarState,
}

impl<T> SelectChapter<'_, T>
where
    T: Novel + Send + Sync,
{
    pub fn new(
        sender: mpsc::Sender<ReadNovelMsg<T>>,
        chapters: Option<Vec<String>>,
        current_chapter: usize,
    ) -> Self {
        let sender_clone = sender.clone();
        let chapters = chapters.unwrap_or_default();

        // 创建时指点当前选择的章节
        let mut state = ListState::default();
        state.select(Some(current_chapter));

        Self {
            scrollbar_state: ScrollbarState::new(chapters.len()),
            list: List::new(chapters).highlight_style(Style::new().bold().on_light_cyan().black()),
            state,
            search: Search::new(
                "搜索章节",
                move |query| {
                    sender_clone
                        .try_send(ReadNovelMsg::QueryChapters(query))
                        .unwrap();
                },
                |_| (true, ""),
            ),
            sender,
        }
    }

    pub fn set_list(&mut self, chapters: Vec<String>, selected: Option<usize>) {
        self.state = ListState::default();
        self.state.select(selected);
        self.scrollbar_state = ScrollbarState::new(chapters.len());
        self.list = self.list.clone().items(chapters);
    }
}

#[async_trait]
impl<T> Component for SelectChapter<'_, T>
where
    T: Novel + Send + Sync,
{
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        let [top, content] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);

        let block = Block::bordered()
            .title(Line::from("目录").centered())
            .border_style(Style::new().dim())
            .padding(Padding::horizontal(1));
        self.search.render(frame, top)?;

        if self.list.is_empty() {
            frame.render_widget(Empty::new("暂无章节"), block.inner(content));
        } else {
            frame.render_stateful_widget(&self.list, block.inner(content), &mut self.state);
        }

        frame.render_widget(block, content);

        self.scrollbar_state = self
            .scrollbar_state
            .position(self.state.selected().unwrap_or(0));

        frame.render_stateful_widget(Scrollbar::default(), content, &mut self.scrollbar_state);
        Ok(())
    }

    async fn handle_key_event(&mut self, key: KeyEvent, state: State) -> Result<Option<KeyEvent>> {
        if key.kind != crossterm::event::KeyEventKind::Press {
            return Ok(Some(key));
        }
        let Some(key) = self.search.handle_key_event(key, state).await? else {
            return Ok(None);
        };

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
                if let Some(chapter_index) = self.state.selected() {
                    self.sender
                        .send(ReadNovelMsg::SelectChapter(chapter_index))
                        .await
                        .unwrap();
                }

                Ok(None)
            }
            _ => {
                return Ok(Some(key));
            }
        }
    }
}
