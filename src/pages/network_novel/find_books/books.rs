use crate::{
    app::State,
    components::{Component, Empty, Loading},
    novel::network_novel::NetworkNovel,
    pages::network_novel::book_detail::BookDetail,
    Navigator, THEME_CONFIG,
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use parse_book_source::{BookList, BookSourceParser};
use ratatui::{
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph, Scrollbar, ScrollbarState, Wrap},
};
use std::sync::Arc;
use tokio::sync::Mutex;
use tui_widget_list::{ListBuilder, ListState, ListView};

pub struct Books {
    pub state: ListState,
    pub books: Option<BookList>,
    pub title: String,
    pub empty_tip: String,
    pub loading: Loading,
    pub is_loading: bool,
    pub page: usize,
    pub navigator: Navigator,
    pub book_source: Arc<Mutex<BookSourceParser>>,
}

impl Books {
    pub fn set_books(&mut self, books: BookList) {
        self.books = Some(books);
    }

    pub fn new(
        navigator: Navigator,
        title: &str,
        empty_tip: &str,
        loading: Loading,
        is_loading: bool,
        book_source: Arc<Mutex<BookSourceParser>>,
    ) -> Self {
        Self {
            state: ListState::default(),
            books: None,
            title: title.to_string(),
            empty_tip: empty_tip.to_string(),
            loading,
            is_loading,
            page: 0,
            navigator,
            book_source,
        }
    }

    pub fn set_title(&mut self, title: &str) {
        self.title = title.to_string();
    }

    pub fn set_empty_tip(&mut self, empty_tip: &str) {
        self.empty_tip = empty_tip.to_string();
    }

    pub fn set_loading(&mut self, loading: Loading, is_loading: bool) {
        self.loading = loading;
        self.is_loading = is_loading;
    }

    fn render_list(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let mut block = Block::bordered()
            .title(
                Line::from(self.title.clone())
                    .style(THEME_CONFIG.basic.border_title)
                    .centered(),
            )
            .border_style(THEME_CONFIG.basic.border);

        if let Some(books) = self.books.as_ref() {
            block = block.title_bottom(
                Line::from(format!(
                    " 第{}页 {}/{}",
                    self.page,
                    self.state.selected.unwrap_or(0) + 1,
                    books.len()
                ))
                .style(THEME_CONFIG.basic.border_info)
                .left_aligned(),
            );
        }

        let inner_area = block.inner(area);

        if self.is_loading {
            frame.render_widget(&self.loading, inner_area);
            frame.render_widget(block, area);
        } else if let Some(books) = &self.books {
            let list_items = books.clone();

            let builder = ListBuilder::new(move |context| {
                let item = list_items[context.index].clone();

                let block = if context.is_selected {
                    Block::bordered()
                        .padding(Padding::horizontal(2))
                        .style(THEME_CONFIG.selected)
                } else {
                    Block::bordered().padding(Padding::horizontal(2))
                };

                let text_style = if context.is_selected {
                    THEME_CONFIG.basic.text.patch(THEME_CONFIG.selected)
                } else {
                    THEME_CONFIG.basic.text
                };

                let mut text = vec![];

                if !item.book_info.name.is_empty() {
                    text.push(
                        Line::from(vec![
                            Span::from("名称：").style(THEME_CONFIG.basic.border_info),
                            Span::from(item.book_info.name),
                        ])
                        .style(text_style),
                    );
                }
                if !item.book_info.author.is_empty() {
                    text.push(
                        Line::from(vec![
                            Span::from("作者：").style(THEME_CONFIG.basic.border_info),
                            Span::from(item.book_info.author),
                        ])
                        .style(text_style),
                    );
                }

                if !item.book_info.kind.is_empty() {
                    text.push(
                        Line::from(vec![
                            Span::from("类型：").style(THEME_CONFIG.basic.border_info),
                            Span::from(item.book_info.kind),
                        ])
                        .style(text_style),
                    );
                }

                if !item.book_info.last_chapter.is_empty() {
                    text.push(
                        Line::from(vec![
                            Span::from("最新章节：").style(THEME_CONFIG.basic.border_info),
                            Span::from(item.book_info.last_chapter),
                        ])
                        .style(text_style),
                    );
                }

                if !item.book_info.word_count.is_empty() {
                    text.push(
                        Line::from(vec![
                            Span::from("字数：").style(THEME_CONFIG.basic.border_info),
                            Span::from(item.book_info.word_count),
                        ])
                        .style(text_style),
                    );
                }

                if !item.book_info.intro.is_empty() {
                    text.push(
                        Line::from(vec![
                            Span::from("简介：").style(THEME_CONFIG.basic.border_info),
                            Span::from(item.book_info.intro),
                        ])
                        .style(text_style),
                    );
                }

                let paragraph = Paragraph::new(text).wrap(Wrap { trim: true }).block(block);
                let height = paragraph.line_count(inner_area.width) as u16;
                (paragraph, height)
            });
            let widget = ListView::new(builder, books.len()).infinite_scrolling(false);
            frame.render_stateful_widget(widget, inner_area, &mut self.state);

            frame.render_widget(block, area);

            if books.len() * 5 > inner_area.height as usize {
                let mut scrollbar_state =
                    ScrollbarState::new(books.len()).position(self.state.selected.unwrap_or(0));
                frame.render_stateful_widget(Scrollbar::default(), area, &mut scrollbar_state);
            }
        } else {
            frame.render_widget(Empty::new(&self.empty_tip), inner_area);
            frame.render_widget(block, area);
        }
    }
}

#[async_trait]
impl Component for Books {
    fn render(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> crate::Result<()> {
        self.render_list(frame, area);
        Ok(())
    }
    async fn handle_key_event(
        &mut self,
        key: KeyEvent,
        state: State,
    ) -> crate::Result<Option<KeyEvent>> {
        if key.kind != KeyEventKind::Press {
            return Ok(Some(key));
        }
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.state.next();
                Ok(None)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.state.previous();
                Ok(None)
            }
            KeyCode::Enter => {
                let index = self.state.selected.ok_or("请选择书籍")?;
                let book_list_item = self
                    .books
                    .as_ref()
                    .ok_or("暂无书籍可以阅读")?
                    .get(index)
                    .ok_or("您选择的书籍不存在")?;

                let novel = match NetworkNovel::from_url(
                    &book_list_item.book_url,
                    state.book_sources,
                )
                .await
                {
                    Ok(novel) => novel,
                    Err(_) => NetworkNovel::new(book_list_item.clone(), self.book_source.clone()),
                };

                self.navigator.push(BookDetail::to_page_route(novel))?;

                Ok(None)
            }
            _ => Ok(Some(key)),
        }
    }

    async fn handle_tick(&mut self, _state: State) -> crate::Result<()> {
        if self.is_loading {
            self.loading.state.calc_next();
        }
        Ok(())
    }
}
