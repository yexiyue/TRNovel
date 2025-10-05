use crate::{
    components::{
        list_view::{ListView, RenderItem},
        widget_wrapper::WidgetWrapper,
    },
    hooks::UseThemeConfig,
};
use crossterm::event::{Event, KeyCode, KeyEventKind};
use ratatui::widgets::Block;
use ratatui_kit::{AnyElement, Hooks, Props, UseEvents, UseState, component, element};
use tui_widget_list::ListState;

#[derive(Default, Props)]
pub struct ListSelectProps<T>
where
    T: Into<WidgetWrapper> + Sync + Send + Clone + 'static,
{
    pub items: Vec<T>,
    pub on_select: ratatui_kit::Handler<'static, T>,
    pub default_value: Option<usize>,
    pub top_title: Option<ratatui::text::Line<'static>>,
    pub bottom_title: Option<ratatui::text::Line<'static>>,
    pub is_editing: bool,
    pub render_item: RenderItem<'static>,
}

#[component]
fn ListSelect<T>(props: &mut ListSelectProps<T>, mut hooks: Hooks) -> impl Into<AnyElement<'static>>
where
    T: Into<WidgetWrapper> + Unpin + Sync + Clone + Send + 'static,
{
    let theme = hooks.use_theme_config();
    let state = hooks.use_state(|| {
        let mut state = ListState::default();
        state.select(props.default_value);
        state
    });

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
    ))
}
