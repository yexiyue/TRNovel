use crate::components::modal::shortcut_info_modal::ShortcutInfoModal;
use crate::{
    components::{WarningModal, select::Select},
    errors::Errors,
    hooks::{UseInitState, UseThemeConfig},
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use parse_book_source::{BookSource, BookSourceParser, ExploreItem};
use ratatui::{
    layout::{Constraint, Margin},
    style::{Style, Stylize},
    text::Line,
    widgets::{ListItem, ListState},
};
use ratatui_kit::prelude::*;
use std::hash::Hash;
mod find_book;
use find_book::*;

#[derive(Debug, Clone, Hash)]
pub struct ExploreListItem(pub ExploreItem);

impl From<ExploreListItem> for ListItem<'_> {
    fn from(value: ExploreListItem) -> Self {
        ListItem::new(value.0.title.clone())
    }
}

#[component]
pub fn SelectBooks(mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let mut info_modal_open = hooks.use_state(|| false);
    let book_source = hooks.use_route_state::<BookSource>();
    let mut explores = hooks.use_state(std::vec::Vec::new);
    let theme = hooks.use_theme_config();

    let list_state = hooks.use_state(ListState::default);
    let mut is_explore_open = hooks.use_state(|| false);

    let mut current_explore = hooks.use_state(|| None::<ExploreListItem>);

    let (book_source_parser, _, error) = hooks.use_init_state(async move {
        let mut res = BookSourceParser::new((*book_source).clone())?;
        let book_source_explores = res.get_explores().await?;

        if let Some(explore) = book_source_explores.first() {
            current_explore.set(Some(ExploreListItem(explore.clone())));
            list_state.write().select_first();
        }

        explores.set(book_source_explores);
        Ok::<BookSourceParser, Errors>(res)
    });

    hooks.use_events(move |event| {
        if let Event::Key(key) = event
            && key.kind == KeyEventKind::Press
        {
            match key.code {
                KeyCode::Tab => {
                    is_explore_open.set(!is_explore_open.get());
                }
                KeyCode::Char('i') | KeyCode::Char('I') => {
                    info_modal_open.set(!info_modal_open.get());
                }
                _ => {}
            }
        }
    });

    let explores_list = hooks.use_memo(
        move || {
            explores
                .read()
                .iter()
                .map(|item| ExploreListItem(item.clone()))
                .collect::<Vec<_>>()
        },
        explores.read().len(),
    );

    let books_list_props = FindBooksProps {
        parser: book_source_parser,
        current_explore: current_explore.read().clone(),
        is_editing: !is_explore_open.get(),
    };

    element!(View {
        FindBooks(..books_list_props)
        WarningModal(
            tip: format!("{:?}", error.read().as_ref()),
            is_error: error.read().is_some(),
            open: error.read().is_some(),
        )
        ShortcutInfoModal(
            key_shortcut_info: vec![
                ("切换分类弹窗", "Tab"),
                ("上下移动", "J / K / ↑ / ↓"),
                ("选择/进入", "Enter"),
                ("上一页", "H / ←"),
                ("下一页", "L / →"),
                ("搜索书籍", "S"),
            ],
            open: info_modal_open.get(),
        )
        Modal(
            width: Constraint::Length(30),
            height: Constraint::Percentage(70),
            style: Style::default().dim(),
            open: is_explore_open.get(),
        ) {
            View(
                margin: Margin::new(1,1),
            ){
                Select<ExploreListItem>(
                    items: explores_list.clone(),
                    state: list_state,
                    on_select: move |item:ExploreListItem| {
                        current_explore.set(Some(item.clone()));
                        is_explore_open.set(false);
                    },
                    top_title: Line::from("选择分类").style(theme.highlight.not_dim()).centered(),
                    is_editing: is_explore_open.get(),
                    empty_message: "暂无分类".to_string(),
                )
            }
        }
    })
}
