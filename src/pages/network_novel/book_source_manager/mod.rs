use crate::{
    app::State,
    book_source::BookSourceCache,
    components::{Component, Confirm, ConfirmState, Empty, KeyShortcutInfo, Loading},
    errors::Errors,
    pages::Page,
    utils::time_to_string,
    Events, Navigator, Result, Router,
};
use async_trait::async_trait;

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use import::Import;
use parse_book_source::BookSource;
use ratatui::{
    style::{Style, Stylize},
    text::{Line, Text},
    widgets::{Block, Padding, Paragraph, Scrollbar, ScrollbarState},
};
use std::sync::Arc;
use tokio::sync::{mpsc::Sender, Mutex};
use tui_widget_list::{ListBuilder, ListState, ListView};

use super::find_books::FindBooks;
pub mod import;

pub enum BookSourceManagerMsg {
    Loaded,
    Error(Errors),
    ParseResult(Vec<BookSource>),
    Parse(String),
    Selected(Vec<BookSource>),
}

pub struct BookSourceManager {
    pub is_loading: bool,
    pub loading: Loading,
    pub state: ListState,
    pub confirm_state: ConfirmState,
    pub book_sources: Arc<Mutex<BookSourceCache>>,
    pub navigator: Navigator,
    pub sender: Sender<BookSourceManagerMsg>,
    pub import: Import,
    pub show_import: bool,
}

impl BookSourceManager {
    pub fn new(
        book_sources: Arc<Mutex<BookSourceCache>>,
        navigator: Navigator,
        sender: Sender<BookSourceManagerMsg>,
    ) -> Self {
        Self {
            is_loading: true,
            book_sources,
            state: ListState::default(),
            confirm_state: ConfirmState::default(),
            navigator,
            loading: Loading::new("加载中..."),
            import: Import::new(sender.clone()),
            sender,
            show_import: false,
        }
    }

    fn render_list(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let list_items = self.book_sources.try_lock().unwrap().clone();
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
impl Page for BookSourceManager {
    type Msg = BookSourceManagerMsg;

    async fn init(
        _arg: (),
        sender: tokio::sync::mpsc::Sender<BookSourceManagerMsg>,
        navigator: Navigator,
        state: State,
    ) -> Result<Self> {
        let book_source = state.book_sources.clone();
        let sender_clone = sender.clone();

        tokio::spawn(async move {
            if let Err(e) = book_source.lock().await.load() {
                sender_clone
                    .send(BookSourceManagerMsg::Error(e))
                    .await
                    .unwrap();
            }
            sender_clone
                .send(BookSourceManagerMsg::Loaded)
                .await
                .unwrap();
        });

        Ok(Self::new(state.book_sources.clone(), navigator, sender))
    }

    async fn update(&mut self, msg: Self::Msg) -> Result<()> {
        match msg {
            BookSourceManagerMsg::Loaded => {
                self.is_loading = false;

                // 加载完成后，如果为空自动打开导入页面
                if self.book_sources.lock().await.is_empty() {
                    self.show_import = true;
                }
            }
            BookSourceManagerMsg::Error(e) => {
                return Err(e);
            }
            BookSourceManagerMsg::ParseResult(book_sources) => {
                self.import.set_book_sources(book_sources);
                self.import.set_loading(false);
            }
            BookSourceManagerMsg::Parse(query) => {
                self.import.set_loading(true);
                let sender = self.sender.clone();
                tokio::spawn(async move {
                    match if query.starts_with("http") {
                        BookSource::from_url(query.trim()).await
                    } else {
                        BookSource::from_path(query.trim())
                    } {
                        Ok(book_sources) => {
                            sender
                                .send(BookSourceManagerMsg::ParseResult(book_sources))
                                .await
                                .unwrap();
                        }
                        Err(err) => {
                            sender
                                .send(BookSourceManagerMsg::Error(err.into()))
                                .await
                                .ok();
                        }
                    }
                });
            }
            BookSourceManagerMsg::Selected(selected_book_sources) => {
                for i in selected_book_sources {
                    self.book_sources.lock().await.add_book_source(i);
                }
                self.import.set_book_sources(vec![]);
                self.show_import = false;
            }
        }
        Ok(())
    }
}

#[async_trait]
impl Component for BookSourceManager {
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        let block = Block::bordered()
            .title(
                Line::from(if self.show_import {
                    "导入书源"
                } else {
                    "书源管理"
                })
                .centered(),
            )
            .border_style(Style::new().dim());

        let container_area = block.inner(area);

        if self.is_loading {
            frame.render_widget(&self.loading, container_area);
            return Ok(());
        }

        if self.book_sources.try_lock().unwrap().is_empty() {
            frame.render_widget(Empty::new("暂无书源，请添加书源"), container_area);
            frame.render_widget(block, area);
        } else if self.show_import {
            self.import.render(frame, container_area)?;
            frame.render_widget(block, area);
        } else {
            self.render_list(frame, container_area);

            let len = self.book_sources.try_lock()?.len();
            let current = self.state.selected.unwrap_or(0);

            frame.render_widget(
                block.title_bottom(format!(" {}/{}", current + 1, len)),
                area,
            );

            if len * 5 > container_area.height as usize {
                let mut scrollbar_state = ScrollbarState::new(len).position(current);
                frame.render_stateful_widget(Scrollbar::default(), area, &mut scrollbar_state);
            }
        }

        frame.render_stateful_widget(
            Confirm::new("警告", "确认删除该书源吗？"),
            container_area,
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
                            self.book_sources.try_lock().unwrap().remove(index);
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
                        return Err("请选择书源".into());
                    };

                    let item = &self.book_sources.lock().await[index];

                    self.navigator
                        .push(Box::new(FindBooks::to_page_route(item.clone())?))?;

                    Ok(None)
                }
                KeyCode::Char('d') => {
                    if self.state.selected.is_none() {
                        return Err("请选择书源".into());
                    }

                    self.confirm_state.show();
                    Ok(None)
                }
                KeyCode::Tab => {
                    self.show_import = !self.show_import;
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
                ("删除选中的书源", "D"),
                ("切换到选择文件", "Tab"),
            ]
        };
        KeyShortcutInfo::new(data)
    }

    async fn handle_tick(&mut self, _state: State) -> Result<()> {
        if self.is_loading {
            self.loading.state.calc_next();
        }
        Ok(())
    }

    async fn handle_events(&mut self, events: Events, state: State) -> Result<Option<Events>> {
        let Some(events) = (if self.show_import {
            self.import.handle_events(events, state.clone()).await?
        } else {
            Some(events)
        }) else {
            return Ok(None);
        };

        match events {
            Events::KeyEvent(key) => self
                .handle_key_event(key, state)
                .await
                .map(|item| item.map(Events::KeyEvent)),

            Events::Tick => {
                self.handle_tick(state).await?;

                Ok(Some(Events::Tick))
            }
            _ => Ok(Some(events)),
        }
    }
}

impl Router for BookSourceManager {}
