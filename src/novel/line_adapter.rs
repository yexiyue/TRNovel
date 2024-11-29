use std::ops::{Deref, DerefMut};

use super::TxtNovel;
use anyhow::Result;
use ratatui::{
    layout::Size,
    text::Text,
    widgets::{Paragraph, Wrap},
};

#[derive(Debug)]
pub struct LineAdapter<'a> {
    pub inner: TxtNovel,
    pub content_lines: usize,
    pub current_line: usize,
    pub size: Size,
    pub page_size: usize,
    pub paragraph: Paragraph<'a>,
}

impl Deref for LineAdapter<'_> {
    type Target = TxtNovel;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for LineAdapter<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl LineAdapter<'_> {
    pub fn new(mut inner: TxtNovel, size: Size) -> Result<Self> {
        let paragraph = Paragraph::new(Text::from(inner.get_content()?.trim_end().to_string()))
            .wrap(Wrap { trim: true });
        let content_lines = paragraph.line_count(size.width) - size.height as usize;
        let current_line = (content_lines as f64 * inner.line_percent).round() as usize;

        Ok(Self {
            inner,
            paragraph,
            content_lines,
            current_line,
            size,
            page_size: 5,
        })
    }

    pub fn get_content(&mut self) -> Result<()> {
        let content = self.inner.get_content()?.trim_end().to_string();
        self.paragraph = Paragraph::new(Text::from(content)).wrap(Wrap { trim: true });
        self.content_lines = self.paragraph.line_count(self.size.width) - self.size.height as usize;
        Ok(())
    }

    pub fn resize(&mut self, size: Size) {
        self.size = size;
        let percent = self.current_line as f64 / self.content_lines as f64;
        self.content_lines = self.paragraph.line_count(size.width) - size.height as usize;
        self.current_line = (self.content_lines as f64 * percent).round() as usize;
    }

    pub fn scroll_down(&mut self) {
        if self.current_line < self.content_lines {
            self.current_line = self.current_line.saturating_add(1);
        }
    }

    pub fn scroll_up(&mut self) {
        if self.current_line > 0 {
            self.current_line = self.current_line.saturating_sub(1);
        }
    }

    pub fn scroll_to_bottom(&mut self) {
        self.current_line = self.content_lines;
    }

    pub fn scroll_to_top(&mut self) {
        self.current_line = 0;
    }

    pub fn scroll_page_down(&mut self) {
        self.current_line = (self.current_line + self.page_size).min(self.content_lines);
    }

    pub fn scroll_page_up(&mut self) {
        self.current_line = self.current_line.saturating_sub(self.page_size);
    }

    pub fn is_top(&self) -> bool {
        self.current_line == 0
    }

    pub fn is_bottom(&self) -> bool {
        self.current_line == self.content_lines
    }
}

impl Drop for LineAdapter<'_> {
    fn drop(&mut self) {
        let percent = self.current_line as f64 / self.content_lines as f64;
        self.inner.line_percent = percent;
    }
}
