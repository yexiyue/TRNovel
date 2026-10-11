use crate::theme::AppChromeTheme;
use ratatui::{
    text::Line,
    widgets::{ListItem, ListState},
};
use ratatui_kit::{
    components::{Select as KitSelect, SelectProps as KitSelectProps},
    prelude::*,
};

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
    pub default_value: Option<usize>,
    pub highlight_symbol: Option<&'static str>,
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
            default_value: None,
            highlight_symbol: None,
        }
    }
}

#[component]
pub fn Select<T>(props: &mut SelectProps<T>, hooks: Hooks) -> impl Into<AnyElement<'static>>
where
    T: Into<ListItem<'static>> + Sync + Send + Clone + 'static,
{
    let theme = hooks.use_component_theme::<AppChromeTheme>();
    Element::<KitSelect<T>> {
        key: ElementKey::decl(0),
        props: KitSelectProps {
            items: std::mem::take(&mut props.items),
            on_select: props.on_select.take(),
            state: props.state,
            top_title: props.top_title.clone(),
            bottom_title: props.bottom_title.clone(),
            active: props.is_editing,
            default_index: props.default_value,
            empty_message: props.empty_message.clone().into(),
            highlight_symbol: props.highlight_symbol,
            style: Some(theme.text),
            border_style: Some(theme.border),
            highlight_style: Some(theme.selected),
            hover_style: Some(theme.highlight),
            empty_style: Some(theme.empty),
            ..Default::default()
        },
    }
}
