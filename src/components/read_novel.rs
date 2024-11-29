use std::path::PathBuf;

use anyhow::{anyhow, Result};
use crossterm::event::{KeyCode, KeyEventKind};
use ratatui::{
    layout::{Constraint, Layout, Rect, Size},
    style::{Style, Stylize},
    text::{Line, Text},
    widgets::{Block, Clear, List, ListState, Padding, Scrollbar, ScrollbarState},
};
use tokio::sync::mpsc::UnboundedSender;

use crate::{
    app::state::State,
    events::Events,
    history::HistoryItem,
    novel::{LineAdapter, TxtNovel},
    routes::Router,
};

use super::{Component, LoadingPage};

#[derive(Debug)]
pub struct ReadNovel {
    pub novel: LineAdapter<'static>,
    pub list_state: ListState,
    pub scrollbar_state: ScrollbarState,
    pub show_sidebar: bool,
}

impl ReadNovel {
    pub fn new(novel: LineAdapter<'static>) -> Result<Self> {
        let mut list_state = ListState::default();
        list_state.select(Some(novel.current_chapter));

        Ok(Self {
            list_state,
            show_sidebar: true,
            scrollbar_state: ScrollbarState::default()
                .position(novel.current_line)
                .content_length(novel.content_lines),
            novel,
        })
    }

    fn update_scrollbar(&mut self) {
        self.scrollbar_state = self.scrollbar_state.position(self.novel.current_line);
    }

    fn render_content(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let current_chapter = self.novel.chapter_offset[self.novel.current_chapter]
            .0
            .clone();

        let paragraph = self
            .novel
            .paragraph
            .clone()
            .scroll((self.novel.current_line as u16, 0))
            .block(
                Block::bordered()
                    .border_style(Style::new().dim())
                    .padding(Padding::new(1, 1, 0, 1))
                    .title(Line::from(current_chapter).centered()),
            );

        self.scrollbar_state = self
            .scrollbar_state
            .content_length(self.novel.content_lines);

        frame.render_widget(paragraph, area);
        frame.render_stateful_widget(Scrollbar::default(), area, &mut self.scrollbar_state);
    }

    fn render_bottom(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let [left_area, right_area] =
            Layout::horizontal([Constraint::Fill(1), Constraint::Fill(1)]).areas(area);

        let percent =
            ((self.novel.current_chapter as f64 / self.novel.chapter_offset.len() as f64) * 100.0)
                .round();

        let current_time = chrono::Local::now().format("%H:%M").to_string();

        frame.render_widget(
            Line::from(format!(
                "{}/{} 行",
                self.novel.current_line + 1,
                self.novel.content_lines + 1
            ))
            .left_aligned()
            .dim(),
            left_area,
        );
        frame.render_widget(
            Line::from(format!("{}% {}", percent, current_time))
                .right_aligned()
                .dim(),
            right_area,
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
                    x: right_area.x + 2,
                    y: right_area.height - 2,
                    width: right_area.width - 4,
                    height: 1,
                },
            );

            self.render_slider(frame, left_area);
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
        };

        Ok(())
    }

    fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        _tx: UnboundedSender<Events>,
        _state: State,
    ) -> anyhow::Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }
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
                        self.novel.get_content()?;
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
                    if self.novel.is_bottom() {
                        self.novel.next_chapter()?;
                        self.novel.get_content()?;
                        self.novel.current_line = 0;
                        self.update_scrollbar();
                    } else {
                        self.novel.scroll_down();
                        self.update_scrollbar();
                    }
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    if self.novel.is_top() {
                        self.novel.prev_chapter()?;
                        self.novel.get_content()?;
                        self.novel.current_line = self.novel.content_lines;
                        self.update_scrollbar();
                    } else {
                        self.novel.scroll_up();
                        self.update_scrollbar();
                    }
                }
                KeyCode::Char('h') | KeyCode::Left => {
                    self.novel.prev_chapter()?;
                    self.novel.get_content()?;
                    self.novel.current_line = 0;
                    self.update_scrollbar();
                }
                KeyCode::Char('l') | KeyCode::Right => {
                    self.novel.next_chapter()?;
                    self.novel.get_content()?;
                    self.novel.current_line = 0;
                    self.update_scrollbar();
                }
                KeyCode::PageDown => {
                    self.novel.scroll_page_down();
                    self.update_scrollbar();
                }
                KeyCode::PageUp => {
                    self.novel.scroll_page_up();
                    self.update_scrollbar();
                }
                KeyCode::Tab => {
                    self.list_state.select(Some(self.novel.current_chapter));
                    self.show_sidebar = true;
                }

                _ => {}
            }
        }
        Ok(())
    }

    fn handle_events(
        &mut self,
        events: Events,
        tx: UnboundedSender<Events>,
        state: State,
    ) -> Result<()> {
        match events {
            Events::KeyEvent(key) => self.handle_key_event(key, tx, state)?,
            Events::Back | Events::Pop => {
                state.history.lock().unwrap().add(
                    self.novel.path.clone(),
                    HistoryItem::from(&self.novel.inner),
                );
            }
            Events::Resize(width, height) => {
                self.novel.resize(Size::new(width - 4, height - 5));
                self.update_scrollbar();
            }
            _ => {}
        }
        Ok(())
    }
}

impl Router for LoadingPage<ReadNovel, PathBuf> {
    fn init(&mut self, tx: UnboundedSender<Events>, state: State) -> Result<()> {
        let path = self.args.to_path_buf();
        let inner = self.inner.clone();
        tokio::spawn(async move {
            match (|| {
                let tx_novel = TxtNovel::from_path(path)?;
                let size = state
                    .size
                    .lock()
                    .unwrap()
                    .clone()
                    .ok_or(anyhow!("No terminal size found"))?;

                *inner.try_lock()? = Some(ReadNovel::new(LineAdapter::new(
                    tx_novel,
                    Size::new(size.width - 4, size.height - 5),
                )?)?);

                Ok::<_, anyhow::Error>(())
            })() {
                Ok(_) => {}
                Err(e) => {
                    tx.send(Events::Error(e.to_string())).unwrap();
                }
            }
        });
        Ok(())
    }
}
