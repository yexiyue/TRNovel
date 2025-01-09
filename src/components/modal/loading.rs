use ratatui::{
    layout::{Constraint, Flex, Layout, Offset},
    widgets::{Block, Clear, Widget},
};
use throbber_widgets_tui::{Throbber, ThrobberState};

use crate::THEME_SETTING;

#[derive(Debug, Clone)]
pub struct Loading {
    pub tip: String,
    pub state: ThrobberState,
}

impl Loading {
    pub fn new(tip: &str) -> Self {
        Self {
            tip: tip.into(),
            state: ThrobberState::default(),
        }
    }
}

impl Widget for &Loading {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let [vertical] = Layout::vertical([Constraint::Length(5)])
            .flex(Flex::Center)
            .areas(area);
        let [horizontal] = Layout::horizontal([Constraint::Percentage(50)])
            .flex(Flex::Center)
            .areas(vertical);

        Clear.render(area, buf);

        let block = Block::bordered().border_style(THEME_SETTING.loading_modal.border);
        let inner_area = block.inner(horizontal);
        block.render(horizontal, buf);

        Throbber::default()
            .label(self.tip.as_str())
            .throbber_set(throbber_widgets_tui::ASCII)
            .to_line(&self.state)
            .centered()
            .style(THEME_SETTING.loading_modal.text)
            .render(inner_area.offset(Offset { x: 0, y: 1 }), buf);
    }
}
