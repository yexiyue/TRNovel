use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent};
use parse_book_source::ChapterList;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, List, ListState, Padding, Scrollbar, ScrollbarState},
};
use tokio::sync::mpsc;

use crate::{
    app::State,
    components::{Component, Search},
    Result,
};

use super::ReadNovelMsg;

pub struct SelectChapter<'a> {
    pub state: ListState,
    pub list: List<'a>,
    pub search: Search<'a>,
    pub sender: mpsc::Sender<ReadNovelMsg>,
    pub scrollbar_state: ScrollbarState,
}

impl SelectChapter<'_> {
    pub fn new(sender: mpsc::Sender<ReadNovelMsg>, chapters: ChapterList) -> Self {
        let sender_clone = sender.clone();

        Self {
            list: List::new(
                chapters
                    .iter()
                    .map(|x| Line::from(x.chapter_name.clone()))
                    .collect::<Vec<_>>(),
            )
            .highlight_style(Style::new().bold().on_light_cyan().black())
            .block(
                Block::bordered()
                    .title(Line::from("目录").centered())
                    .border_style(Style::new().dim())
                    .padding(Padding::horizontal(1)),
            ),
            state: ListState::default(),
            search: Search::new("搜索章节", move |query| {
                sender_clone
                    .try_send(ReadNovelMsg::QueryChapters(query))
                    .unwrap();
            }),
            sender,
            scrollbar_state: ScrollbarState::new(chapters.len()),
        }
    }

    pub fn set_list(&mut self, chapters: ChapterList) {
        self.state = ListState::default();
        self.list = self.list.clone().items(
            chapters
                .iter()
                .map(|x| Line::from(x.chapter_name.clone()))
                .collect::<Vec<_>>(),
        );
        self.scrollbar_state = ScrollbarState::new(chapters.len());
    }
}

#[async_trait]
impl Component for SelectChapter<'_> {
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        let [top, content] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);
        self.search.render(frame, top)?;

        frame.render_stateful_widget(&self.list, content, &mut self.state);

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
