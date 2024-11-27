use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Style, Stylize},
    text::{Line, Text},
    widgets::{Block, Clear, List, ListState, Padding, Paragraph, Scrollbar, ScrollbarState, Wrap},
};

use crate::novel::TxtNovel;

use super::Component;

#[derive(Debug, Clone)]
pub struct ReadNovel {
    pub novel: TxtNovel,
    pub list_state: ListState,
    pub scrollbar_state: ScrollbarState,
    pub show_sidebar: bool,
    pub content: String,
}

impl ReadNovel {
    pub fn new(mut novel: TxtNovel) -> Result<Self> {
        let mut list_state = ListState::default();
        list_state.select(Some(novel.current_chapter));
        let content = novel.get_content()?;

        Ok(Self {
            list_state,
            show_sidebar: true,
            scrollbar_state: ScrollbarState::new(novel.content_lines).position(novel.current_line),
            novel,
            content,
        })
    }

    fn update_scrollbar(&mut self) {
        self.scrollbar_state = self
            .scrollbar_state
            .position(self.novel.current_line)
            .content_length(self.novel.content_lines);
    }

    fn render_content(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let current_chapter = self.novel.chapter_offset[self.novel.current_chapter]
            .0
            .clone();

        let paragraph = Paragraph::new(Text::from(
            self.content
                .lines()
                .map(|item| Line::from(item))
                .collect::<Vec<_>>(),
        ))
        .wrap(Wrap { trim: true })
        .block(
            Block::bordered()
                .border_style(Style::new().dim())
                .padding(Padding::new(1, 1, 0, 1))
                .title(Line::from(current_chapter).centered()),
        )
        .scroll((self.novel.current_line as u16, 0));

        frame.render_widget(paragraph, area);
        frame.render_stateful_widget(Scrollbar::default(), area, &mut self.scrollbar_state);
    }

    fn render_bottom(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let percent =
            ((self.novel.current_chapter as f64 / self.novel.chapter_offset.len() as f64) * 100.0)
                .round();

        let current_time = chrono::Local::now().format("%H:%M").to_string();

        frame.render_widget(
            Line::from(format!("{}% {}", percent, current_time))
                .right_aligned()
                .dim(),
            area,
        );
    }

    fn render_slider(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let block = Block::bordered()
            .title(Line::from("目录").centered())
            .border_style(Style::new().dim())
            .padding(Padding::horizontal(1));

        let items = self
            .novel
            .chapter_offset
            .iter()
            .map(|item| Text::from(item.0.as_str()))
            .collect::<Vec<_>>();
        let mut scrollbar_state =
            ScrollbarState::new(items.len()).position(self.list_state.selected().unwrap_or(0));
        let list = List::new(items)
            .highlight_style(Style::new().bold().on_light_cyan())
            .block(block);

        frame.render_stateful_widget(list, area, &mut self.list_state);
        frame.render_stateful_widget(Scrollbar::default(), area, &mut scrollbar_state);
    }
}

impl Component for ReadNovel {
    fn draw(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> anyhow::Result<()> {
        frame.render_widget(Clear, area);

        if self.show_sidebar {
            let [left_area, right_area] =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .areas(area);

            self.render_content(frame, right_area);
            self.render_bottom(
                frame,
                Rect {
                    x: right_area.x,
                    y: right_area.height - 2,
                    width: right_area.width - 2,
                    height: 1,
                },
            );

            self.render_slider(frame, left_area);
        } else {
            self.render_content(frame, area);
            self.render_bottom(
                frame,
                Rect {
                    x: area.x,
                    y: area.height - 2,
                    width: area.width - 2,
                    height: 1,
                },
            );
        };

        Ok(())
    }

    fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> anyhow::Result<Option<crate::actions::Actions>> {
        if self.show_sidebar {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    self.list_state.select_next();
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    self.list_state.select_previous();
                }
                KeyCode::Esc => {
                    self.show_sidebar = false;
                }
                KeyCode::Enter => {
                    if let Some(chapter) = self.list_state.selected() {
                        self.novel.set_chapter(chapter)?;
                        self.content = self.novel.get_content()?;
                        self.update_scrollbar();
                    }
                    self.show_sidebar = false;
                }
                KeyCode::Tab => {
                    self.show_sidebar = false;
                }
                _ => {}
            }
        } else {
            match key.code {
                KeyCode::Char('j') | KeyCode::Down | KeyCode::Char('\n' | ' ') => {
                    if self.novel.current_line < self.novel.content_lines {
                        self.novel.current_line = self.novel.current_line.saturating_add(1);
                        self.update_scrollbar();
                    } else {
                        self.content = self.novel.next_chapter()?;
                        self.update_scrollbar();
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if self.novel.current_line > 0 {
                        self.novel.current_line = self.novel.current_line.saturating_sub(1);
                        self.update_scrollbar();
                    } else {
                        self.content = self.novel.prev_chapter()?;
                        self.novel.current_line = self.novel.content_lines;
                        self.update_scrollbar();
                    }
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    self.content = self.novel.prev_chapter()?;
                    self.update_scrollbar();
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    self.content = self.novel.next_chapter()?;
                    self.update_scrollbar();
                }
                KeyCode::Tab => {
                    self.list_state.select(Some(self.novel.current_chapter));
                    self.show_sidebar = true;
                }

                _ => {}
            }
        }
        Ok(None)
    }
}
