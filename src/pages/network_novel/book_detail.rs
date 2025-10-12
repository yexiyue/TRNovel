use anyhow::anyhow;
use crossterm::event::{Event, KeyCode, KeyEventKind};
use parse_book_source::{BookInfo, BookListItem, BookSourceParser};
use ratatui::{
    layout::{Constraint, Margin},
    style::Stylize,
    text::{Line, Span},
    widgets::{Paragraph, Wrap},
};
use ratatui_kit::prelude::*;

use crate::{
    book_source::BookSourceCache,
    components::{Loading, WarningModal},
    errors::Errors,
    hooks::{UseInitState, UseThemeConfig},
    novel::network_novel::NetworkNovel,
};

#[derive(Debug, Clone)]
pub enum BookDetailState {
    New {
        network_novel: Box<BookSourceParser>,
        book_list_item: BookListItem,
    },
    Cache {
        url: String,
    },
}

impl BookDetailState {
    pub fn new(book_list_item: BookListItem, network_novel: BookSourceParser) -> Self {
        Self::New {
            network_novel: Box::new(network_novel),
            book_list_item,
        }
    }

    pub fn from_cache(url: String) -> Self {
        Self::Cache { url }
    }
}

#[component]
pub fn BookDetail(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let book_detail_state = hooks.use_route_state::<BookDetailState>();
    let book_source_cache = *hooks.use_context::<State<Option<BookSourceCache>>>();

    let mut book_info = hooks.use_state(|| None::<BookInfo>);
    let size = hooks.use_previous_size();
    let theme = hooks.use_theme_config();
    let mut navigate = hooks.use_navigate();

    let (book_source_parser, loading, error) = hooks.use_init_state(async move {
        let mut novel = match &(*book_detail_state) {
            BookDetailState::New {
                network_novel,
                book_list_item,
            } => NetworkNovel::new(book_list_item.clone(), *network_novel.clone()),
            BookDetailState::Cache { url } => {
                let book_source_cache = book_source_cache
                    .read()
                    .clone()
                    .ok_or(anyhow!("没有书源缓存"))?;
                NetworkNovel::from_url(url, &book_source_cache)?
            }
        };

        let res = novel
            .book_source
            .lock()
            .await
            .get_book_info(&novel.book_list_item.book_url)
            .await?;

        novel.set_book_info(&res);

        book_info.set(Some(res));

        Ok::<NetworkNovel, Errors>(novel)
    });

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                    if let Some(novel) = book_source_parser.read().clone() {
                        navigate.push_with_state("/network-novel", novel);
                    }
                }
                _ => {}
            }
        }
    });

    let book_info = book_info.read().clone().unwrap_or_default();

    let title = vec![
        Span::from("名称：").style(theme.detail_info),
        Span::from(book_info.name.clone()).style(theme.basic.text),
    ];
    let author = vec![
        Span::from("作者：").style(theme.detail_info),
        Span::from(book_info.author.clone()).style(theme.basic.text),
    ];
    let kind = vec![
        Span::from("类型：").style(theme.detail_info),
        Span::from(book_info.kind.clone()).style(theme.basic.text),
    ];
    let word_count = vec![
        Span::from("字数：").style(theme.detail_info),
        Span::from(book_info.word_count.clone()).style(theme.basic.text),
    ];

    let last_chapter = vec![
        Span::from("最新章节：").style(theme.detail_info),
        Span::from(book_info.last_chapter.clone()).style(theme.basic.text),
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
    ];

    let paragraph = Paragraph::new(text).wrap(Wrap { trim: true });

    let intro = Paragraph::new(vec![
        Line::from("简介：").style(theme.detail_info),
        Line::from(book_info.intro),
    ])
    .wrap(Wrap { trim: true });

    element!(Border(
        top_title: Line::from("小说详情").centered().style(theme.highlight.not_dim()),
        border_style: theme.basic.border,
    ){
        #(if loading.get(){
            element!(Loading(tip: "加载中...")).into_any()
        }else{
            element!(ScrollView(
                gap:1,
                scroll_bars:ScrollBars{
                    vertical_scrollbar_visibility: ScrollbarVisibility::Always,
                    ..Default::default()
                }
            ){
                View(height:Constraint::Length(paragraph.line_count(size.width.saturating_sub(4)) as u16), margin: Margin::new(1,0)){
                    Text(
                        text: paragraph,
                        style: theme.basic.text,
                    )
                }
                View(height:Constraint::Length(intro.line_count(size.width.saturating_sub(4)) as u16),margin: Margin::new(1,0)){
                    Text(
                        text: intro,
                        style: theme.basic.text,
                    )
                }
            }).into_any()
        })
        WarningModal(
            tip: format!("{:?}", error.read().as_ref()),
            is_error: error.read().is_some(),
            open: error.read().is_some(),
        )
    })
}
