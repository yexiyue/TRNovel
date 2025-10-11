use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint},
    text::Line,
    widgets::{Block, Scrollbar},
};
use ratatui_kit::prelude::*;
use std::path::PathBuf;
use tui_tree_widget::{TreeItem, TreeState};

use crate::hooks::UseThemeConfig;

#[derive(Default, Props)]
pub struct FileSelectProps {
    pub items: Vec<TreeItem<'static, PathBuf>>,
    pub on_select: Handler<'static, PathBuf>,
    pub default_value: Option<usize>,
    pub top_title: Option<Line<'static>>,
    pub bottom_title: Option<Line<'static>>,
    pub is_editing: bool,
    pub empty_message: String,
}

#[component]
pub fn FileSelect(props: &mut FileSelectProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let state = hooks.use_state(TreeState::default);
    let theme = hooks.use_theme_config();

    let is_empty = props.items.is_empty();

    let mut on_select = props.on_select.take();

    hooks.use_events({
        let is_editing = props.is_editing;
        move |event| {
            if let Event::Key(key) = event
                && key.kind == KeyEventKind::Press
                && is_editing
            {
                match key.code {
                    KeyCode::Char('h') | KeyCode::Left => {
                        state.write().key_left();
                    }
                    KeyCode::Char('j') | KeyCode::Down => {
                        state.write().key_down();
                    }
                    KeyCode::Char('k') | KeyCode::Up => {
                        state.write().key_up();
                    }
                    KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                        let res: Option<PathBuf> = state.read().selected().last().cloned();
                        if let Some(path) = res {
                            state.write().toggle_selected();
                            if path.is_file() {
                                on_select(path);
                            }
                        }
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

    element!(TreeSelect<PathBuf>(
        style: theme.basic.text,
        highlight_style: theme.selected,
        state: state,
        items: props.items.clone(),
        scrollbar: Scrollbar::default(),
        block: border,
    ))
    .into_any()
}
