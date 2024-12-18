use ratatui::{
    layout::{Constraint, Flex, Layout},
    style::Stylize,
    widgets::{Paragraph, Widget, Wrap},
};

#[derive(Debug, Clone)]
pub struct Empty {
    pub text: String,
}

impl Empty {
    pub fn new(text: &str) -> Self {
        Self {
            text: text.to_string(),
        }
    }
}

impl Widget for Empty {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
    where
        Self: Sized,
    {
        let [vertical] = Layout::vertical([Constraint::Length(1)])
            .flex(Flex::Center)
            .areas(area);

        Paragraph::new(self.text)
            .yellow()
            .bold()
            .centered()
            .wrap(Wrap { trim: true })
            .render(vertical, buf);
    }
}
