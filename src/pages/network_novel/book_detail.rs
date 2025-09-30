use crate::{
    Navigator, Result, RoutePage, Router, THEME_CONFIG,
    app::State,
    components::{Component, KeyShortcutInfo, LoadingWrapper, LoadingWrapperInit},
    novel::network_novel::NetworkNovel,
    pages::ReadNovel,
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use parse_book_source::BookInfo;
use ratatui::{
    buffer::Buffer,
    layout::{Constraint, Layout, Margin, Size},
    text::{Line, Span},
    widgets::{Block, Padding, Paragraph, Widget, WidgetRef, Wrap},
};
use tui_scrollview::{ScrollView, ScrollViewState, ScrollbarVisibility};

pub struct BookDetail {
    pub book_info: BookInfo,
    pub novel: NetworkNovel,
    pub navigator: Navigator,
    pub other_paragraph: Paragraph<'static>,
    pub intro_paragraph: Paragraph<'static>,
    pub state: ScrollViewState,
}

impl BookDetail {
    pub fn new(book_info: BookInfo, navigator: Navigator, mut novel: NetworkNovel) -> Self {
        let title = vec![
            Span::from("名称：").style(THEME_CONFIG.detail_info),
            Span::from(book_info.name.clone()).style(THEME_CONFIG.basic.text),
        ];
        let author = vec![
            Span::from("作者：").style(THEME_CONFIG.detail_info),
            Span::from(book_info.author.clone()).style(THEME_CONFIG.basic.text),
        ];
        let kind = vec![
            Span::from("类型：").style(THEME_CONFIG.detail_info),
            Span::from(book_info.kind.clone()).style(THEME_CONFIG.basic.text),
        ];
        let word_count = vec![
            Span::from("字数：").style(THEME_CONFIG.detail_info),
            Span::from(book_info.word_count.clone()).style(THEME_CONFIG.basic.text),
        ];

        let last_chapter = vec![
            Span::from("最新章节：").style(THEME_CONFIG.detail_info),
            Span::from(book_info.last_chapter.clone()).style(THEME_CONFIG.basic.text),
        ];

        let text = vec![
            Line::from(title),
            Line::from(""),
            Line::from(author),
            Line::from(""),
            Line::from(kind),
            Line::from(""),
            Line::from(word_count),
            Line::from(""),
            Line::from(last_chapter),
            Line::from(""),
        ];

        let paragraph = Paragraph::new(text).wrap(Wrap { trim: true });

        let intro = Paragraph::new(book_info.intro.clone())
            .style(THEME_CONFIG.basic.text)
            .wrap(Wrap { trim: true });

        novel.set_book_info(&book_info);

        Self {
            book_info,
            navigator,
            other_paragraph: paragraph,
            intro_paragraph: intro,
            state: ScrollViewState::default(),
            novel,
        }
    }
    pub fn to_page_route(novel: NetworkNovel) -> Box<dyn RoutePage> {
        LoadingWrapper::<BookDetail>::route_page("加载书籍详情中...", novel, None)
    }

    fn render_content(&mut self, buf: &mut Buffer) {
        let [top, bottom] = Layout::vertical([
            Constraint::Length(self.other_paragraph.line_count(buf.area.width) as u16),
            Constraint::Length(self.intro_paragraph.line_count(buf.area.width) as u16),
        ])
        .areas(buf.area);

        let [left, right] =
            Layout::horizontal([Constraint::Length(6), Constraint::Fill(1)]).areas(bottom);

        self.other_paragraph.render_ref(top, buf);
        Line::from("简介：")
            .style(THEME_CONFIG.detail_info)
            .render(left, buf);
        self.intro_paragraph.render_ref(right, buf);
    }
}

#[async_trait]
impl Component for BookDetail {
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        let block = Block::bordered()
            .title(
                Line::from("书籍详情")
                    .centered()
                    .style(THEME_CONFIG.basic.border_title),
            )
            .border_style(THEME_CONFIG.basic.border)
            .padding(Padding::new(3, 0, 1, 1));

        let inner_area = block.inner(area).inner(Margin::new(1, 0));
        let height = self.intro_paragraph.line_count(inner_area.width)
            + self.other_paragraph.line_count(inner_area.width);

        let mut scroll_view = ScrollView::new(Size::new(inner_area.width, height as u16))
            .horizontal_scrollbar_visibility(ScrollbarVisibility::Never);

        self.render_content(scroll_view.buf_mut());

        frame.render_stateful_widget(scroll_view, block.inner(area), &mut self.state);
        frame.render_widget(block, area);
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
            KeyCode::Enter | KeyCode::Char('\n' | ' ') => {
                self.navigator
                    .push(Box::new(ReadNovel::<NetworkNovel>::to_page_route(
                        self.novel.clone(),
                    )))?;
                Ok(None)
            }
            KeyCode::Char('j') | KeyCode::Down => {
                self.state.scroll_down();
                Ok(None)
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.state.scroll_up();
                Ok(None)
            }
            _ => Ok(Some(key)),
        }
    }
    fn key_shortcut_info(&self) -> crate::components::KeyShortcutInfo {
        KeyShortcutInfo::new(vec![
            ("向下滚动", "J / ▼"),
            ("向上滚动", "K / ▲"),
            ("进入阅读模式", "Enter"),
        ])
    }
}

#[async_trait]
impl LoadingWrapperInit for BookDetail {
    type Arg = NetworkNovel;
    async fn init(novel: Self::Arg, navigator: Navigator, _state: State) -> Result<Option<Self>> {
        let book_info = novel
            .book_source
            .lock()
            .await
            .get_book_info(&novel.book_list_item.book_url)
            .await?;

        Ok(Some(BookDetail::new(book_info, navigator, novel)))
    }
}

impl Router for BookDetail {}
