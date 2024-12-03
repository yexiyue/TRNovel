use ratatui::{
    layout::{Alignment, Constraint, Flex, Layout, Margin},
    style::{Style, Stylize},
    widgets::{Block, Clear, Paragraph, StatefulWidget, Widget, Wrap},
};

#[derive(Debug, Clone)]
pub struct Confirm {
    pub title: String,
    pub content: String,
}

impl Confirm {
    pub fn new(title: &str, content: &str) -> Self {
        Self {
            title: title.to_string(),
            content: content.to_string(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ConfirmState {
    pub show: bool,
    pub confirm: bool,
}

impl ConfirmState {
    pub fn confirm(&mut self) {
        self.confirm = true;
    }

    pub fn toggle(&mut self) {
        self.confirm = !self.confirm;
    }

    pub fn hide(&mut self) {
        self.show = false;
        self.confirm = false;
    }

    pub fn is_confirm(&self) -> bool {
        self.confirm
    }

    pub fn show(&mut self) {
        self.show = true;
    }
}

impl StatefulWidget for Confirm {
    type State = ConfirmState;
    fn render(
        self,
        area: ratatui::prelude::Rect,
        buf: &mut ratatui::prelude::Buffer,
        state: &mut Self::State,
    ) {
        if state.show {
            Block::new().dim().render(area, buf);

            let [horizontal] = Layout::horizontal([Constraint::Percentage(70)])
                .flex(Flex::Center)
                .areas(area);

            let [block_area] = Layout::vertical([Constraint::Max(10)])
                .flex(Flex::Center)
                .areas(horizontal);

            Clear.render(horizontal, buf);

            let block = Block::bordered()
                .title(self.title.as_str())
                .title_alignment(Alignment::Center)
                .title_style(Style::default().light_yellow().bold())
                .border_style(Style::default().light_yellow());

            let inner_area = block.inner(block_area);

            block.render(block_area.inner(Margin::new(2, 0)), buf);

            let [content_area, bottom_area] =
                Layout::vertical([Constraint::Fill(1), Constraint::Length(3)]).areas(inner_area);

            Paragraph::new(self.content.as_str())
                .yellow()
                .wrap(Wrap { trim: true })
                .centered()
                .render(content_area.inner(Margin::new(2, 2)), buf);

            let [left, right] =
                Layout::horizontal([Constraint::Length(10), Constraint::Length(10)])
                    .flex(Flex::SpaceAround)
                    .areas(bottom_area);

            let (cancel_style, confirm_style) = if state.confirm {
                (Style::default(), Style::default().light_red())
            } else {
                (Style::default().light_cyan(), Style::default())
            };

            Paragraph::new("取消")
                .alignment(Alignment::Center)
                .block(Block::bordered())
                .style(cancel_style)
                .render(left, buf);

            Paragraph::new("确认")
                .alignment(Alignment::Center)
                .block(Block::bordered())
                .style(confirm_style)
                .render(right, buf);
        }
    }
}
