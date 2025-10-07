use crate::{
    components::list_view::{ListView, RenderItem},
    hooks::UseThemeConfig,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::widgets::Block;
use ratatui_kit::{AnyElement, Handler, Hooks, Props, UseEvents, UseState, component, element};
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
    let theme = hooks.use_theme_config();
    let state = hooks.use_state(|| {
        let mut state = ListState::default();
        state.select(props.default_value);
        state
    });

    let state = props.state.unwrap_or(state);

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
                    KeyCode::Char('l') | KeyCode::Right | KeyCode::Enter => {
                        let res = state.read().selected;
                        if let Some(path) = res {
                            on_select(data[path].clone());
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

    element!(ListView(
        state: state,
        block: border,
        render_item: props.render_item.take(),
        item_count: props.items.len(),
    ))
}
