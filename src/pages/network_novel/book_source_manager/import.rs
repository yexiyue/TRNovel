use std::collections::HashSet;

use super::BookSourceManagerMsg;
use crate::{
    app::State,
    components::{Component, Loading, Search},
    utils::time_to_string,
    Events,
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use parse_book_source::BookSource;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Style, Stylize},
    text::{Line, Span, Text},
    widgets::{Block, Clear, Padding, Paragraph, Scrollbar, ScrollbarState, Widget},
};
use tui_widget_list::{ListBuilder, ListState, ListView};

struct ListItem {
    pub book_source: BookSource,
    pub selected: bool,
    pub height_light: bool,
}

impl Widget for ListItem {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let block = if self.height_light {
            Block::bordered()
                .padding(Padding::horizontal(2))
                .light_cyan()
        } else if self.selected {
            Block::bordered().padding(Padding::horizontal(2)).green()
        } else {
            Block::bordered().padding(Padding::horizontal(2))
        };
        let [left, right] = Layout::horizontal([Constraint::Length(1), Constraint::Fill(1)])
            .areas(block.inner(area));

        block.render(area, buf);

        Paragraph::new(Text::from(vec![
            Line::from(self.book_source.book_source_name.clone()).centered(),
            Line::from(
                format!(
                    "{} {}",
                    self.book_source.book_source_url,
                    time_to_string(self.book_source.last_update_time).unwrap()
                )
                .dim(),
            )
            .right_aligned(),
        ]))
        .render(right, buf);

        if self.selected {
            Span::from("✔").render(left, buf);
        } else {
            Span::from("☐").render(left, buf);
        }
    }
}

pub struct Import {
    pub loading: Loading,
    pub is_loading: bool,
    pub sender: tokio::sync::mpsc::Sender<BookSourceManagerMsg>,
    pub book_sources: Vec<BookSource>,
    pub selected: HashSet<usize>,
    pub list_state: ListState,
    pub search: Search<'static>,
}

impl Import {
    pub fn new(sender: tokio::sync::mpsc::Sender<BookSourceManagerMsg>) -> Self {
        let sender_clone = sender.clone();
        Self {
            search: Search::new("请输入书源链接或文件地址", move |query| {
                sender_clone
                    .try_send(BookSourceManagerMsg::Parse(query))
                    .unwrap()
            }),
            loading: Loading::new("解析中..."),
            is_loading: false,
            sender,
            book_sources: vec![],
            selected: HashSet::new(),
            list_state: ListState::default(),
        }
    }

    pub fn set_loading(&mut self, loading: bool) {
        self.is_loading = loading;
    }

    pub fn set_book_sources(&mut self, book_sources: Vec<BookSource>) {
        self.book_sources = book_sources;
        self.list_state.select(None);
        self.selected.clear();
    }

    fn render_list(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let list_items = self.book_sources.clone();
        let selected = self.selected.clone();
        let length = list_items.len();
        let builder = ListBuilder::new(move |context| {
            let item = &list_items[context.index];

            let list_item = ListItem {
                book_source: item.clone(),
                selected: selected.contains(&context.index),
                height_light: context.is_selected,
            };

            (list_item, 4)
        });
        let widget = ListView::new(builder, length).infinite_scrolling(false);
        frame.render_stateful_widget(widget, area, &mut self.list_state);
    }
}

#[async_trait]
impl Component for Import {
    fn render(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> crate::Result<()> {
        frame.render_widget(Clear, area);

        if self.is_loading {
            frame.render_widget(&self.loading, area);
            return Ok(());
        }
        let [top, bottom] =
            Layout::vertical([Constraint::Length(3), Constraint::Fill(1)]).areas(area);

        self.search.render(frame, top)?;

        let len = self.book_sources.len();
        let current = self.list_state.selected.unwrap_or(0);

        let mut block = Block::bordered()
            .title(Line::from("请选择要导入的书源").centered())
            .border_style(Style::new().dim());

        if len != 0 {
            block = block.title_bottom(format!(" {}/{}", current + 1, len));
        }

        let list_area = block.inner(bottom);
        self.render_list(frame, list_area);

        frame.render_widget(block, bottom);

        if len * 4 > list_area.height as usize {
            let mut scrollbar_state = ScrollbarState::new(len).position(current);
            frame.render_stateful_widget(Scrollbar::default(), bottom, &mut scrollbar_state);
        }

        Ok(())
    }

    async fn handle_tick(&mut self, _state: State) -> crate::Result<()> {
        if self.is_loading {
            self.loading.state.calc_next();
        }
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
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.list_state.next();
                Ok(None)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.list_state.previous();
                Ok(None)
            }
            KeyCode::Char('\n' | ' ') => {
                if let Some(item) = self.list_state.selected {
                    if self.selected.contains(&item) {
                        self.selected.remove(&item);
                    } else {
                        self.selected.insert(item);
                    }
                }
                Ok(None)
            }
            KeyCode::Enter => {
                let mut selected_book_source: Vec<BookSource> = vec![];
                for item in self.selected.iter() {
                    selected_book_source.push(self.book_sources[*item].clone());
                }

                self.sender
                    .send(BookSourceManagerMsg::Selected(selected_book_source))
                    .await
                    .unwrap();

                Ok(None)
            }
            _ => Ok(Some(key)),
        }
    }

    async fn handle_events(
        &mut self,
        events: Events,
        state: State,
    ) -> crate::Result<Option<Events>> {
        let Some(events) = self.search.handle_events(events, state.clone()).await? else {
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
