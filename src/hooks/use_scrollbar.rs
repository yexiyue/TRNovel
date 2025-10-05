use ratatui::{
    layout::{Margin, Rect},
    widgets::{Scrollbar, ScrollbarState},
};
use ratatui_kit::{Hook, Hooks};

pub trait UseScrollbar {
    fn use_scrollbar(&mut self, content_length: usize, position: Option<usize>);
}

#[derive(Debug, Default)]
pub struct UseScrollbarImpl {
    area: Rect,
    content_length: usize,
    position: Option<usize>,
}

impl Hook for UseScrollbarImpl {
    fn pre_component_draw(&mut self, drawer: &mut ratatui_kit::ComponentDrawer) {
        self.area = drawer.area;
    }

    fn post_component_draw(&mut self, drawer: &mut ratatui_kit::ComponentDrawer) {
        let height = drawer.area.height;

        if self.content_length as u16 > height && self.content_length > 0 {
            let content_len = (self.content_length as f32 / (height as f32)) as usize;
            let position =
                (self.position.unwrap_or_default() as f32 / (height as f32)).floor() as usize;
            let mut scrollbar_state = ScrollbarState::default()
                .content_length(content_len)
                .position(position);

            drawer.render_stateful_widget(
                Scrollbar::default(),
                self.area.inner(Margin::new(0, 1)),
                &mut scrollbar_state,
            );
        }
    }
}

impl UseScrollbar for Hooks<'_, '_> {
    fn use_scrollbar(&mut self, content_length: usize, position: Option<usize>) {
        let scrollbar = self.use_hook(UseScrollbarImpl::default);
        scrollbar.content_length = content_length;
        scrollbar.position = position;
    }
}
