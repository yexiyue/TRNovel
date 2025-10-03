use ratatui::{
    layout::Rect,
    widgets::{Scrollbar, ScrollbarState},
};
use ratatui_kit::{Hook, Hooks};

pub trait UseScrollbar {
    fn use_scrollbar(&mut self, content_length: usize, position: Option<usize>);
}

#[derive(Debug, Default)]
pub struct UseScrollbarImpl {
    pub scrollbar_state: ScrollbarState,
    area: Rect,
}

impl Hook for UseScrollbarImpl {
    fn pre_component_draw(&mut self, drawer: &mut ratatui_kit::ComponentDrawer) {
        self.area = drawer.area;
    }

    fn post_component_draw(&mut self, drawer: &mut ratatui_kit::ComponentDrawer) {
        drawer.render_stateful_widget(Scrollbar::default(), self.area, &mut self.scrollbar_state);
    }
}

impl UseScrollbar for Hooks<'_, '_> {
    fn use_scrollbar(&mut self, content_length: usize, position: Option<usize>) {
        let scrollbar = self.use_hook(UseScrollbarImpl::default);

        let scrollbar_state = ScrollbarState::default()
            .content_length(content_length)
            .position(position.unwrap_or_default());

        scrollbar.scrollbar_state = scrollbar_state;
    }
}
