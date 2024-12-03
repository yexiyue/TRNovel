use ratatui::{
    layout::Alignment,
    style::{Style, Styled},
    text::Line,
    widgets::{Block, Borders, Widget, WidgetRef},
};

#[derive(Debug, Clone, Default)]
pub struct Cell<'a> {
    pub content: Line<'a>,
    pub is_last: bool,
}

impl<'a, T> From<T> for Cell<'a>
where
    T: Into<Line<'a>>,
{
    fn from(value: T) -> Self {
        Self {
            content: value.into(),
            ..Default::default()
        }
    }
}

impl<'a> Cell<'a> {
    pub fn new<T>(content: T, is_last: bool) -> Self
    where
        T: Into<Line<'a>>,
    {
        Self {
            content: content.into(),
            is_last,
        }
    }

    pub fn style<S: Into<Style>>(mut self, style: S) -> Self {
        self.content = self.content.set_style(style);
        self
    }

    pub fn centered(mut self) -> Self {
        self.content = self.content.centered();
        self
    }

    pub fn right_aligned(mut self) -> Self {
        self.content = self.content.right_aligned();
        self
    }

    pub fn left_aligned(mut self) -> Self {
        self.content = self.content.left_aligned();
        self
    }

    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.content = self.content.alignment(alignment);
        self
    }
}

impl WidgetRef for Cell<'_> {
    fn render_ref(&self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        let block = Block::bordered().borders(if self.is_last {
            Borders::LEFT
        } else {
            Borders::LEFT | Borders::RIGHT
        });
        let inner_area = block.inner(area);
        block.render_ref(area, buf);
        self.content.render_ref(inner_area, buf)
    }
}

impl Widget for Cell<'_> {
    fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer) {
        self.render_ref(area, buf);
    }
}

impl<'a> Styled for Cell<'a> {
    type Item = Self;
    fn style(&self) -> ratatui::prelude::Style {
        self.content.style
    }

    fn set_style<S: Into<ratatui::prelude::Style>>(self, style: S) -> Self::Item {
        self.style(style)
    }
}
