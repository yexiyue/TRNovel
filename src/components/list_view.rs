use ratatui::widgets::{Widget, WidgetRef};
use ratatui::{
    style::Style,
    widgets::{Block, Clear},
};
use ratatui_kit::{Component, Props, State};
use std::{ops::Deref, sync::Arc};
use tui_widget_list::{
    ListBuildContext, ListBuilder, ListState, ListView as TuiListView, ScrollAxis,
};

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

pub struct RenderItem<'a>(
    bool,
    Box<dyn Fn(&ListBuildContext) -> (WidgetWrapper, u16) + Sync + Send + 'a>,
);

impl<'a> RenderItem<'a> {
    pub fn call(&self, ctx: &ListBuildContext) -> (WidgetWrapper, u16) {
        (self.1)(ctx)
    }

    pub fn is_default(&self) -> bool {
        self.0
    }

    pub fn take(&mut self) -> Self {
        std::mem::take(self)
    }
}

impl<'a> Default for RenderItem<'a> {
    fn default() -> Self {
        Self(true, Box::new(|_ctx: &ListBuildContext| (Clear.into(), 0)))
    }
}

impl<F> From<F> for RenderItem<'_>
where
    F: Fn(&ListBuildContext) -> (WidgetWrapper, u16) + Sync + Send + 'static,
{
    fn from(f: F) -> Self {
        Self(false, Box::new(f))
    }
}

#[derive(Default, Props)]
pub struct ListView {
    pub state: Option<State<ListState>>,
    pub scroll_axis: ScrollAxis,
    pub style: Style,
    pub block: Option<Block<'static>>,
    pub scroll_padding: u16,
    pub infinite_scrolling: bool,
    pub render_item: RenderItem<'static>,
    pub item_count: usize,
}

impl Component for ListView {
    type Props<'a>
        = ListView
    where
        Self: 'a;
    fn new(props: &Self::Props<'_>) -> Self {
        Self {
            state: props.state.clone(),
            scroll_axis: props.scroll_axis,
            style: props.style,
            block: props.block.clone(),
            scroll_padding: props.scroll_padding,
            infinite_scrolling: props.infinite_scrolling,
            render_item: Default::default(),
            item_count: props.item_count,
        }
    }

    fn update(
        &mut self,
        props: &mut Self::Props<'_>,
        _hooks: ratatui_kit::Hooks,
        _updater: &mut ratatui_kit::ComponentUpdater,
    ) {
        *self = Self {
            state: props.state.clone(),
            scroll_axis: props.scroll_axis,
            style: props.style,
            block: props.block.clone(),
            scroll_padding: props.scroll_padding,
            infinite_scrolling: props.infinite_scrolling,
            render_item: props.render_item.take(),
            item_count: props.item_count,
        }
    }

    fn draw(&mut self, drawer: &mut ratatui_kit::ComponentDrawer<'_, '_>) {
        let render_item = self.render_item.take();

        let list_builder = ListBuilder::new(|ctx: &ListBuildContext| render_item.call(ctx));

        let mut list = TuiListView::new(list_builder, self.item_count)
            .style(self.style)
            .infinite_scrolling(self.infinite_scrolling)
            .scroll_axis(self.scroll_axis)
            .scroll_padding(self.scroll_padding);

        if let Some(block) = &self.block {
            list = list.block(block.clone());
        }

        if let Some(state) = &mut self.state {
            drawer.render_stateful_widget(list, drawer.area, &mut state.write_no_update());
        }
    }
}
