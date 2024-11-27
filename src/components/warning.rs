use ratatui::{
    layout::{Alignment, Constraint, Flex, Layout, Offset},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Clear, Widget},
};

#[derive(Debug, Clone)]
pub struct Warning {
    pub tip: String,
}

impl Warning {
    pub fn new(tip: &str) -> Self {
        Self { tip: tip.into() }
    }
}

impl Widget for &mut Warning {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let [vertical] = Layout::vertical([Constraint::Length(5)])
            .flex(Flex::Center)
            .areas(area);
        let [horizontal] = Layout::horizontal([Constraint::Percentage(50)])
            .flex(Flex::Center)
            .areas(vertical);

        Clear.render(area, buf);
        let block = Block::bordered();
        let inner_area = block.inner(horizontal);
        block.render(horizontal, buf);

        Line::from(self.tip.as_str())
            .bold()
            .centered()
            .light_yellow()
            .render(inner_area.offset(Offset { x: 0, y: 1 }), buf);
    }
}

impl Widget for Warning {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let [vertical] = Layout::vertical([Constraint::Length(5)])
            .flex(Flex::Center)
            .areas(area);
        let [horizontal] = Layout::horizontal([Constraint::Percentage(50)])
            .flex(Flex::Center)
            .areas(vertical);

        Clear.render(area, buf);
        let block = Block::bordered()
            .title("警告".light_yellow())
            .title_alignment(Alignment::Center)
            .title_bottom("按ESC键继续".dim())
            .title_alignment(Alignment::Center)
            .border_style(Style::new().yellow());
        let inner_area = block.inner(horizontal);
        block.render(horizontal, buf);

        Line::from(self.tip.as_str())
            .bold()
            .centered()
            .light_yellow()
            .render(inner_area.offset(Offset { x: 0, y: 1 }), buf);
    }
}
