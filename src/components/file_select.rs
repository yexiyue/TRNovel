use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint},
    text::Line,
    widgets::{Block, Scrollbar},
};
use ratatui_kit::prelude::*;
use std::path::PathBuf;
use tui_tree_widget::{TreeItem, TreeState};

use crate::theme::AppChromeTheme;

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
    let theme = hooks.use_component_theme::<AppChromeTheme>();

    let is_empty = props.items.is_empty();
    let item_ids = collect_identifiers(&props.items);

    // 搜索或目录扫描结果变化后,旧选中路径可能已经不在当前树中。
    // 清空选中和展开状态,避免下一次 Enter 沿用被筛掉的旧文件。
    hooks.use_effect(
        move || {
            state.write().select(Vec::new());
            state.write().close_all();
        },
        item_ids,
    );

    let mut on_select = props.on_select.take();
    let items = props.items.clone();

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, {
        let is_editing = props.is_editing;
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press || !is_editing {
                return EventResult::Ignored;
            }
            match key.code {
                KeyCode::Char('h') | KeyCode::Left => {
                    state.write().key_left();
                    EventResult::Consumed
                }
                KeyCode::Char('j') | KeyCode::Down => {
                    state.write().key_down();
                    EventResult::Consumed
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    state.write().key_up();
                    EventResult::Consumed
                }
                KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                    let res = selected_visible_path(&state.read(), &items);
                    if let Some(path) = res {
                        state.write().toggle_selected();
                        if path.is_file() {
                            on_select(path);
                        }
                    }
                    EventResult::Consumed
                }
                _ => EventResult::Ignored,
            }
        }
    });

    let mut border = Block::bordered().border_style(theme.border);

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
                border_style: theme.border,
            ){
                Center(
                    height:Constraint::Length(5),
                    width:Constraint::Percentage(50)
                ){
                    Text(
                        text: props.empty_message.clone(),
                        alignment: Alignment::Center,
                        style: theme.empty,
                        wrap: true,
                    )
                }
            }
        )
        .into_any();
    }

    element!(TreeSelect<PathBuf>(
        style: theme.text,
        highlight_style: theme.selected,
        state: state,
        items: props.items.clone(),
        scrollbar: Scrollbar::default(),
        block: border,
    ))
    .into_any()
}

fn collect_identifiers(items: &[TreeItem<'static, PathBuf>]) -> Vec<PathBuf> {
    let mut identifiers = Vec::new();
    for item in items {
        identifiers.push(item.identifier().clone());
        identifiers.extend(collect_identifiers(item.children()));
    }
    identifiers
}

fn selected_visible_path(
    state: &TreeState<PathBuf>,
    items: &[TreeItem<'static, PathBuf>],
) -> Option<PathBuf> {
    let selected = state.selected().last()?;
    contains_identifier(items, selected).then(|| selected.clone())
}

fn contains_identifier(items: &[TreeItem<'static, PathBuf>], target: &PathBuf) -> bool {
    items
        .iter()
        .any(|item| item.identifier() == target || contains_identifier(item.children(), target))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stale_selection_is_rejected_after_filtering() {
        let a = PathBuf::from("A.txt");
        let b = PathBuf::from("B.txt");
        let items = vec![TreeItem::new_leaf(a.clone(), "A.txt")];
        let mut state = TreeState::default();
        state.select(vec![b]);

        assert_eq!(selected_visible_path(&state, &items), None);

        state.select(vec![a.clone()]);
        assert_eq!(selected_visible_path(&state, &items), Some(a));
    }
}
