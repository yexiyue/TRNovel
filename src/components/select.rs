use crate::hooks::{UseScrollbar, UseThemeConfig};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint},
    text::Line,
    widgets::{List, ListItem, ListState},
};
use ratatui_kit::prelude::*;

#[derive(Props)]
pub struct SelectProps<T>
where
    T: Into<ListItem<'static>> + Sync + Send + Clone,
{
    pub items: Vec<T>,
    pub on_select: Handler<'static, T>,
    pub state: Option<State<ListState>>,
    pub top_title: Option<Line<'static>>,
    pub bottom_title: Option<Line<'static>>,
    pub is_editing: bool,
    pub empty_message: String,
}

impl<T> Default for SelectProps<T>
where
    T: Into<ListItem<'static>> + Sync + Send + Clone,
{
    fn default() -> Self {
        Self {
            items: vec![],
            on_select: Handler::default(),
            state: None,
            top_title: None,
            bottom_title: None,
            is_editing: false,
            empty_message: "暂无数据".to_string(),
        }
    }
}

#[component]
pub fn Select<T>(props: &mut SelectProps<T>, mut hooks: Hooks) -> impl Into<AnyElement<'static>>
where
    T: Into<ListItem<'static>> + Sync + Send + Clone + 'static,
{
    let state = hooks.use_state(ListState::default);
    let state = props.state.unwrap_or(state);

    let theme = hooks.use_theme_config();
    let is_empty = props.items.is_empty();

    let list = List::new(props.items.clone())
        .style(theme.basic.text)
        .highlight_style(theme.selected);

    let mut on_select = props.on_select.take();

    hooks.use_scrollbar(list.len(), state.read().selected());

    hooks.use_events({
        let items = props.items.clone();
        let is_editing = props.is_editing;
        move |event| {
            if let Event::Key(key) = event
                && key.kind == KeyEventKind::Press
                && is_editing
            {
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => {
                        state.write().select_next();
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        state.write().select_previous();
                    }
                    KeyCode::Enter => {
                        if let Some(index) = state.read().selected() {
                            on_select(items[index].clone());
                        }
                    }
                    _ => {}
                }
            }
        }
    });

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
        );
    }

    element!(Border(
        border_style: theme.basic.border,
        top_title:props.top_title.clone(),
        bottom_title:props.bottom_title.clone()
    ){
        $(list,state)
    })
}
