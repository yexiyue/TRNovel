use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint},
    text::Line,
    widgets::{List, ListItem, ListState},
};
use ratatui_kit::{
    AnyElement, Handler, Hooks, Props, UseEffect, UseEvents, UseState, component, element,
    prelude::{Border, Center, Text},
};

use crate::hooks::{UseScrollbar, UseThemeConfig};

#[derive(Default, Props)]
pub struct SelectProps<T>
where
    T: Into<ListItem<'static>> + Sync + Send + Clone,
{
    pub items: Vec<T>,
    pub on_select: Handler<'static, T>,
    pub value: Option<usize>,
    pub top_title: Option<Line<'static>>,
    pub bottom_title: Option<Line<'static>>,
    pub is_editing: bool,
    pub empty_message: String,
}

#[component]
pub fn Select<T>(props: &mut SelectProps<T>, mut hooks: Hooks) -> impl Into<AnyElement<'static>>
where
    T: Into<ListItem<'static>> + Sync + Send + Clone + 'static,
{
    let state = hooks.use_state(|| ListState::default().with_selected(props.value));
    let theme = hooks.use_theme_config();
    let is_empty = props.items.is_empty();

    hooks.use_effect(
        || {
            state.write().select(props.value);
        },
        props.value,
    );

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
