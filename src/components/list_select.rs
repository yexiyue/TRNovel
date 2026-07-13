use crate::{
    components::{
        Loading,
        list_view::{ListView, RenderItem},
    },
    theme::AppChromeTheme,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::{
    layout::{Alignment, Constraint},
    widgets::Block,
};
use ratatui_kit::prelude::*;
use tui_widget_list::ListState;

#[derive(Props)]
pub struct ListSelectProps<T>
where
    T: Sync + Send + Clone + 'static,
{
    pub items: Vec<T>,
    pub on_select: ratatui_kit::Handler<'static, T>,
    pub default_value: Option<usize>,
    pub top_title: Option<ratatui::text::Line<'static>>,
    pub bottom_title: Option<ratatui::text::Line<'static>>,
    pub is_editing: bool,
    pub render_item: RenderItem<'static>,
    pub state: Option<ratatui_kit::State<ListState>>,
    pub empty_message: String,
    pub loading: bool,
    pub loading_tip: String,
}

impl<T> Default for ListSelectProps<T>
where
    T: Sync + Send + Clone + 'static,
{
    fn default() -> Self {
        Self {
            items: vec![],
            on_select: Handler::default(),
            default_value: None,
            top_title: None,
            bottom_title: None,
            is_editing: false,
            render_item: RenderItem::default(),
            state: None,
            empty_message: String::default(),
            loading: false,
            loading_tip: String::from("加载中..."),
        }
    }
}

#[component]
pub fn ListSelect<T>(
    props: &mut ListSelectProps<T>,
    mut hooks: Hooks,
) -> impl Into<AnyElement<'static>>
where
    T: Unpin + Sync + Clone + Send + 'static,
{
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    let state = hooks.use_state(|| {
        let mut state = ListState::default();
        state.select(props.default_value);
        state
    });
    let is_empty = props.items.is_empty();

    let state = props.state.unwrap_or(state);

    hooks.use_event_handler(EventScope::Current, EventPriority::Normal, {
        let is_editing = props.is_editing;
        let mut on_select = props.on_select.take();
        let data = props.items.clone();
        move |event| {
            let Event::Key(key) = event else {
                return EventResult::Ignored;
            };
            if key.kind != KeyEventKind::Press || !is_editing {
                return EventResult::Ignored;
            }
            match key.code {
                KeyCode::Char('j') | KeyCode::Down => {
                    state.write().next();
                    EventResult::Consumed
                }
                KeyCode::Char('k') | KeyCode::Up => {
                    state.write().previous();
                    EventResult::Consumed
                }
                KeyCode::Enter => {
                    // 用 data.get(path) 而非裸 data[path]:列表可能为空但 state.selected 仍是
                    // 强制的 Some(0)(如空书源列表按 Tab 进只选模式、或书源删到空),
                    // 裸索引会 index out of bounds panic 直接崩溃退出。
                    let res = state.read().selected;
                    if let Some(path) = res
                        && let Some(item) = data.get(path)
                    {
                        on_select(item.clone());
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

    if props.loading {
        return element!(
            Border(
                top_title: props.top_title.clone(),
                bottom_title: props.bottom_title.clone(),
                border_style: theme.border,
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

    element!(ListView(
        state: state,
        block: border,
        render_item: props.render_item.take(),
        item_count: props.items.len(),
    ))
    .into_any()
}
