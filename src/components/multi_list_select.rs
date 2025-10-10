use std::collections::HashSet;

use crate::{
    components::{
        Loading,
        list_view::{ListView, RenderItem},
    },
    hooks::UseThemeConfig,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint},
    widgets::Block,
};
use ratatui_kit::{
    AnyElement, Handler, Hooks, Props, State, UseEvents, UseState, component, element,
    prelude::{Border, Center, Text},
};
use tui_widget_list::ListState;

#[derive(Props)]
pub struct MultiListSelectProps<T>
where
    T: Sync + Send + Clone + 'static,
{
    pub items: Vec<T>,
    pub on_select: ratatui_kit::Handler<'static, Vec<T>>,
    pub top_title: Option<ratatui::text::Line<'static>>,
    pub bottom_title: Option<ratatui::text::Line<'static>>,
    pub is_editing: bool,
    pub render_item: RenderItem<'static>,
    pub empty_message: String,
    pub value: Option<State<HashSet<usize>>>,
    pub loading: bool,
    pub loading_tip: String,
}

impl<T> Default for MultiListSelectProps<T>
where
    T: Sync + Send + Clone + 'static,
{
    fn default() -> Self {
        Self {
            items: vec![],
            on_select: Handler::default(),
            top_title: None,
            bottom_title: None,
            is_editing: false,
            render_item: RenderItem::default(),
            empty_message: String::default(),
            value: None,
            loading: false,
            loading_tip: String::from("加载中..."),
        }
    }
}

#[component]
pub fn MultiListSelect<T>(
    props: &mut MultiListSelectProps<T>,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>>
where
    T: Unpin + Sync + Clone + Send + 'static,
{
    let theme = hooks.use_theme_config();
    let state = hooks.use_state(ListState::default);
    let selected = hooks.use_state(HashSet::<usize>::default);
    let selected = props.value.unwrap_or(selected);

    let is_empty = props.items.is_empty();

    hooks.use_events({
        let is_editing = props.is_editing;
        let mut on_select = props.on_select.take();
        let data = props.items.clone();
        move |event| {
            if let Event::Key(key) = event
                && key.kind == KeyEventKind::Press
                && is_editing
            {
                match key.code {
                    KeyCode::Char('h') | KeyCode::Left => {
                        state.write().select(None);
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        state.write().next();
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        state.write().previous();
                    }
                    KeyCode::Char('l')
                    | KeyCode::Right
                    | KeyCode::Char('\n')
                    | KeyCode::Char(' ') => {
                        if let Some(item) = state.read().selected {
                            let is_included = selected.read().contains(&item);
                            if is_included {
                                selected.write().remove(&item);
                            } else {
                                selected.write().insert(item);
                            }
                        }
                    }
                    KeyCode::Enter => {
                        on_select(selected.read().iter().map(|&i| data[i].clone()).collect());
                    }
                    _ => {}
                }
            }
        }
    });

    let mut border = Block::bordered().border_style(theme.basic.border);

    if let Some(title) = props.top_title.clone() {
        border = border.title_top(title);
    }
    if let Some(title) = props.bottom_title.clone() {
        border = border.title_bottom(title);
    }

    if props.loading {
        return element!(
            Border(
                top_title: props.top_title.clone(),
                bottom_title: props.bottom_title.clone(),
                border_style: theme.basic.border,
            ){
                Loading(tip: props.loading_tip.clone())
            }
        )
        .into_any();
    }

    if is_empty {
        return element!(
            Border(
                top_title: props.top_title.clone(),
                bottom_title: props.bottom_title.clone(),
                border_style: theme.basic.border,
            ){
                Center(
                    height:Constraint::Length(5),
                    width:Constraint::Percentage(50)
                ){
                    Text(
                        text: props.empty_message.clone(),
                        alignment: Alignment::Center,
                        style: theme.colors.warning_color,
                        wrap: true,
                    )
                }
            }
        )
        .into_any();
    }

    element!(ListView(
        state: state,
        block: border,
        render_item: props.render_item.take(),
        item_count: props.items.len(),
    ))
    .into_any()
}
