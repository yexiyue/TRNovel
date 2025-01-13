use ratatui::{
    layout::{Alignment, Constraint, Flex, Layout, Margin},
    style::Stylize,
    text::Line,
    widgets::{Block, Clear, Padding, Paragraph, Widget, Wrap},
};

use crate::THEME_CONFIG;

#[derive(Debug, Clone)]
pub struct Warning {
    pub tip: String,
    pub is_error: bool,
}

impl Warning {
    pub fn new(tip: &str, is_error: bool) -> Self {
        Self {
            tip: tip.into(),
            is_error,
        }
    }
}

impl Widget for Warning {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let [vertical] = Layout::vertical([Constraint::Length(8)])
            .flex(Flex::Center)
            .areas(area);
        let [horizontal] = Layout::horizontal([Constraint::Percentage(70)])
            .flex(Flex::Center)
            .areas(vertical);

        if self.is_error {
            Clear.render(area, buf);
        } else {
            Clear.render(horizontal, buf);
        }

        let block = if self.is_error {
            Block::bordered()
                .title(Line::from("错误").style(THEME_CONFIG.error_modal.border_title))
                .title_alignment(Alignment::Center)
                .title_bottom(Line::from("按q退出").style(THEME_CONFIG.error_modal.border_info))
                .title_alignment(Alignment::Center)
                .border_style(THEME_CONFIG.error_modal.border)
                .padding(Padding::uniform(1))
        } else {
            Block::bordered()
                .title(Line::from("警告").style(THEME_CONFIG.warning_modal.border_title))
                .title_alignment(Alignment::Center)
                .title_bottom(
                    Line::from("按ESC键继续").style(THEME_CONFIG.warning_modal.border_info),
                )
                .title_alignment(Alignment::Center)
                .border_style(THEME_CONFIG.warning_modal.border)
                .padding(Padding::uniform(1))
        };

        Block::new().dim().render(area, buf);

        Paragraph::new(self.tip)
            .centered()
            .style(if self.is_error {
                THEME_CONFIG.error_modal.text
            } else {
                THEME_CONFIG.warning_modal.text
            })
            .wrap(Wrap { trim: true })
            .block(block)
            .not_dim()
            .render(horizontal.inner(Margin::new(2, 0)), buf);
    }
}
