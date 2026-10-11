use ratatui::widgets::{Widget, WidgetRef};
use ratatui::{style::Style, widgets::Block};
use ratatui_kit::prelude::*;
use std::{ops::Deref, sync::Arc};
use tui_widget_list::{ListState, ScrollAxis};

#[derive(Clone)]
pub struct WidgetWrapper {
    widget: Arc<dyn WidgetRef + Send + Sync>,
}

impl Widget for WidgetWrapper {
    fn render(self, area: ratatui::layout::Rect, buf: &mut ratatui::buffer::Buffer) {
        self.widget.render_ref(area, buf);
    }
}

impl<T> From<T> for WidgetWrapper
where
    T: WidgetRef + Send + Sync + 'static,
{
    fn from(widget: T) -> Self {
        Self {
            widget: Arc::new(widget),
        }
    }
}

impl Deref for WidgetWrapper {
    type Target = dyn WidgetRef + Send + Sync;

    fn deref(&self) -> &Self::Target {
        &*self.widget
    }
}

pub type RenderItem<'a> = RenderVirtualItem<'a, WidgetWrapper>;

#[derive(Default, Props)]
pub struct ListViewProps {
    pub state: Option<State<ListState>>,
    pub scroll_axis: ScrollAxis,
    pub style: Style,
    pub block: Option<Block<'static>>,
    pub scroll_padding: u16,
    pub infinite_scrolling: bool,
    pub render_item: RenderItem<'static>,
    pub item_count: usize,
}

/// 业务列表只保留渲染适配;键盘、点击确认/勾选由上层选择组件处理。
#[component]
pub fn ListView(props: &mut ListViewProps, mut hooks: Hooks) -> impl Into<AnyElement<'static>> {
    let local_state = hooks.use_state(ListState::default);
    let state = props.state.unwrap_or(local_state);
    // 框架首次挂载会同步 default_index;保留业务已设置的初始游标。
    let default_index = hooks.use_state(|| state.read().selected).get();
    element!(VirtualList<WidgetWrapper>(
        state: state,
        item_count: props.item_count,
        active: false,
        default_index: default_index,
        render_item: std::mem::take(&mut props.render_item),
        scroll_axis: props.scroll_axis,
        style: props.style,
        block: props.block.clone(),
        scroll_padding: props.scroll_padding,
        infinite_scrolling: props.infinite_scrolling,
    ))
}
