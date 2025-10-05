use std::{ops::Deref, sync::Arc};

use ratatui::widgets::{Widget, WidgetRef};

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
