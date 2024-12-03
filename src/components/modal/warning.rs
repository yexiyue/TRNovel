use ratatui::{
    layout::{Alignment, Constraint, Flex, Layout, Margin},
    style::{Style, Stylize},
    widgets::{Block, Clear, Padding, Paragraph, Widget, Wrap},
};

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
                .title("错误".light_red())
                .title_alignment(Alignment::Center)
                .title_bottom("按q退出".dim())
                .title_alignment(Alignment::Center)
                .border_style(Style::new().light_red())
                .padding(Padding::uniform(1))
        } else {
            Block::bordered()
                .title("警告".light_yellow())
                .title_alignment(Alignment::Center)
                .title_bottom("按ESC键继续".dim())
                .title_alignment(Alignment::Center)
                .border_style(Style::new().yellow())
                .padding(Padding::uniform(1))
        };

        Block::new().dim().render(area, buf);

        Paragraph::new(self.tip)
            .centered()
            .style(if self.is_error {
                Style::new().light_red()
            } else {
                Style::new().light_yellow()
            })
            .bold()
            .wrap(Wrap { trim: true })
            .block(block)
            .not_dim()
            .render(horizontal.inner(Margin::new(2, 0)), buf);
    }
}
