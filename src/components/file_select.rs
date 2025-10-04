use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::text::Line;
use ratatui_kit::{
    AnyElement, Handler, Hooks, Props, UseEvents, UseState, component, element,
    prelude::{Border, TreeSelect},
};
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
}

#[component]
pub fn FileSelect(props: &mut FileSelectProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let state = hooks.use_state(|| TreeState::default());
    let theme = hooks.use_theme_config();

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

    element!(Border(
        border_style: theme.basic.border,
        top_title: props.top_title.clone(),
        bottom_title: props.bottom_title.clone()
    ){
        TreeSelect<PathBuf>(
            style: theme.basic.text,
            highlight_style: theme.selected,
            state: Some(state.clone()),
            items: props.items.clone(),
        )
    })
}
