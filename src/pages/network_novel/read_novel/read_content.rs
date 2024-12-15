use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use parse_book_source::Chapter;
use ratatui::{
    layout::{Constraint, Layout, Rect, Size},
    style::{Style, Stylize},
    text::{Line, Text},
    widgets::{Block, Padding, Paragraph, Scrollbar, ScrollbarState, Wrap},
};
use tokio::sync::mpsc;

use crate::{
    app::State,
    components::{Component, Loading},
    Events, Result,
};

use super::ReadNovelMsg;

pub struct ReadContent<'a> {
    pub loading: Loading,
    pub is_loading: bool,
    pub content_lines: usize,
    pub current_line: usize,
    pub size: Size,
    pub page_size: usize,
    pub paragraph: Option<Paragraph<'a>>,
    pub current_chapter: Option<Chapter>,
    pub chapter_percent: f64,
    pub sender: mpsc::Sender<ReadNovelMsg>,
}

impl ReadContent<'_> {
    pub fn new(size: Size, sender: mpsc::Sender<ReadNovelMsg>, is_loading: bool) -> Result<Self> {
        Ok(Self {
            paragraph: None,
            content_lines: 0,
            current_line: 0,
            size,
            page_size: 5,
            loading: Loading::new("加载中..."),
            is_loading,
            current_chapter: None,
            chapter_percent: 0.0,
            sender,
        })
    }

    pub fn set_loading(&mut self, is_loading: bool) {
        self.is_loading = is_loading;
    }

    pub fn set_page_size(&mut self, page_size: usize) {
        self.page_size = page_size;
    }

    pub fn set_current_line(&mut self, current_line: usize) {
        self.current_line = current_line;
    }

    pub fn set_current_chapter(&mut self, chapter: Chapter) {
        self.current_chapter = Some(chapter);
    }

    pub fn set_chapter_percent(&mut self, chapter_percent: f64) {
        self.chapter_percent = chapter_percent;
    }

    pub fn set_content(&mut self, content: String) {
        let paragraph = Paragraph::new(Text::from(content)).wrap(Wrap { trim: true });

        let lines = paragraph.line_count(self.size.width);
        self.content_lines = lines
            .saturating_sub(self.size.height as usize)
            .max(self.size.height as usize);

        self.paragraph = Some(paragraph);
    }

    pub fn resize(&mut self, size: Size) {
        self.size = size;
        if let Some(paragraph) = &self.paragraph {
            let percent = self.current_line as f64 / self.content_lines as f64;
            let lines = paragraph.line_count(size.width);
            self.content_lines = lines
                .saturating_sub(size.height as usize)
                .max(self.size.height as usize);
            self.current_line = (self.content_lines as f64 * percent).round() as usize;
        }
    }

    pub fn scroll_down(&mut self) {
        if self.current_line < self.content_lines {
            self.current_line = self.current_line.saturating_add(1);
        }
    }

    pub fn scroll_up(&mut self) {
        if self.current_line > 0 {
            self.current_line = self.current_line.saturating_sub(1);
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.current_line = self.content_lines;
    }

    pub fn scroll_to_top(&mut self) {
        self.current_line = 0;
    }

    pub fn scroll_page_down(&mut self) {
        self.current_line = (self.current_line + self.page_size).min(self.content_lines);
    }

    pub fn scroll_page_up(&mut self) {
        self.current_line = self.current_line.saturating_sub(self.page_size);
    }

    pub fn is_top(&self) -> bool {
        self.current_line == 0
    }

    pub fn is_bottom(&self) -> bool {
        self.current_line == self.content_lines
    }

    fn render_content(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let current_chapter = self
            .current_chapter
            .clone()
            .map(|item| item.chapter_name)
            .unwrap_or_default();

        let paragraph = self
            .paragraph
            .clone()
            .unwrap()
            .scroll((self.current_line as u16, 0))
            .block(
                Block::bordered()
                    .border_style(Style::new().dim())
                    .padding(Padding::new(1, 1, 0, 1))
                    .title(Line::from(current_chapter).centered()),
            );

        let mut scrollbar_state =
            ScrollbarState::new(self.content_lines).position(self.current_line);

        frame.render_widget(paragraph, area);
        frame.render_stateful_widget(Scrollbar::default(), area, &mut scrollbar_state);
    }

    fn render_bottom(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let [left_area, right_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(area);

        let current_time = chrono::Local::now().format("%H:%M").to_string();

        frame.render_widget(
            Line::from(format!(
                "{}/{} 行",
                self.current_line + 1,
                self.content_lines + 1
            ))
            .left_aligned()
            .dim(),
            left_area,
        );
        frame.render_widget(
            Line::from(format!("{:.2}% {}", self.chapter_percent, current_time))
                .right_aligned()
                .dim(),
            right_area,
        );
    }
}

#[async_trait]
impl Component for ReadContent<'_> {
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        if self.is_loading {
            frame.render_widget(&self.loading, area);
        } else {
            self.render_content(frame, area);
            self.render_bottom(
                frame,
                Rect {
                    x: area.x + 2,
                    y: area.height - 2,
                    width: area.width - 4,
                    height: 1,
                },
            );
        }

        Ok(())
    }

    async fn handle_tick(&mut self, _state: State) -> Result<()> {
        if self.is_loading {
            self.loading.state.calc_next();
        }
        Ok(())
    }

    async fn handle_key_event(&mut self, key: KeyEvent, _state: State) -> Result<Option<KeyEvent>> {
        if key.kind != KeyEventKind::Press || self.is_loading {
            return Ok(Some(key));
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('\n' | ' ') => {
                if self.is_bottom() {
                    self.sender.send(ReadNovelMsg::Next).await.unwrap();
                } else {
                    self.scroll_down();
                }
                Ok(None)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                if self.is_top() {
                    self.sender.send(ReadNovelMsg::ScrollToPrev).await.unwrap();
                } else {
                    self.scroll_up();
                }
                Ok(None)
            }
            KeyCode::Char('h') | KeyCode::Left => {
                self.sender.send(ReadNovelMsg::Prev).await.unwrap();
                Ok(None)
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.sender.send(ReadNovelMsg::Next).await.unwrap();
                Ok(None)
            }
            KeyCode::PageDown => {
                self.scroll_page_down();
                Ok(None)
            }
            KeyCode::PageUp => {
                self.scroll_page_up();
                Ok(None)
            }

            _ => Ok(Some(key)),
        }
    }

    async fn handle_events(&mut self, events: Events, state: State) -> Result<Option<Events>> {
        match events {
            Events::KeyEvent(key) => self
                .handle_key_event(key, state)
                .await
                .map(|item| item.map(Events::KeyEvent)),

            Events::Tick => {
                self.handle_tick(state).await?;

                Ok(Some(Events::Tick))
            }
            Events::Resize(w, h) => {
                self.resize(Size::new(w, h));
                Ok(Some(Events::Resize(w, h)))
            }
            _ => Ok(Some(events)),
        }
    }
}
