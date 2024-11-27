use super::Component;
use anyhow::Result;
use crossterm::event::{Event, KeyCode};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Tabs},
};
pub use select_file::SelectFile;
pub use select_history::SelectHistory;
use strum::{Display, EnumCount, EnumIter, FromRepr, IntoEnumIterator};

mod empty;
pub mod select_file;
pub mod select_history;

#[derive(Debug)]
pub struct SelectNovel<'a> {
    pub select_file: SelectFile<'a>,
    pub select_history: SelectHistory,
    pub mode: Mode,
    pub show_info: bool,
}

#[derive(Debug, Clone, EnumIter, FromRepr, Default, Copy, Display, EnumCount)]
pub enum Mode {
    #[default]
    #[strum(to_string = "选择文件")]
    SelectFile,
    #[strum(to_string = "历史记录")]
    SelectHistory,
}

impl Mode {
    pub fn toggle(self) -> Self {
        let current_index = self as usize;
        let next_index = current_index.saturating_add(1) % Mode::COUNT;
        Self::from_repr(next_index).unwrap_or(self)
    }
}

impl<'a> Component for SelectNovel<'a> {
    fn draw(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> anyhow::Result<()> {
        let [title, content_area] =
            Layout::vertical(vec![Constraint::Length(1), Constraint::Min(1)]).areas(area);
        self.render_tabs(frame, title);
        let block = Block::bordered();
        let inner_area = block.inner(content_area);
        frame.render_widget(block, content_area);
        self.render_tab(frame, inner_area)?;
        Ok(())
    }

    fn handle_term_events(
        &mut self,
        event: crossterm::event::Event,
    ) -> anyhow::Result<Option<crate::actions::Actions>> {
        match event {
            Event::Key(key) => {
                self.handle_key_event(key)?;
            }
            _ => {}
        };
        match self.mode {
            Mode::SelectFile => self.select_file.handle_term_events(event),
            Mode::SelectHistory => self.select_history.handle_term_events(event),
        }
    }

    fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
    ) -> anyhow::Result<Option<crate::actions::Actions>> {
        match key.code {
            KeyCode::Tab => {
                self.mode = self.mode.toggle();
            }
            KeyCode::Char('i') => {
                self.show_info = !self.show_info;
            }
            _ => {}
        }
        Ok(None)
    }
}

impl<'a> SelectNovel<'a> {
    pub fn new(select_file: SelectFile<'a>, select_history: SelectHistory) -> Result<Self> {
        Ok(Self {
            select_file,
            select_history,
            mode: Mode::default(),
            show_info: false,
        })
    }
    fn render_tabs(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) {
        let titles = Mode::iter().map(|item| Line::from(item.to_string()));
        let select_tab_index = self.mode as usize;
        let tabs = Tabs::new(titles)
            .select(select_tab_index)
            .highlight_style(Style::new().green());
        frame.render_widget(tabs, area);
    }

    fn render_tab(
        &mut self,
        frame: &mut ratatui::Frame,
        area: ratatui::prelude::Rect,
    ) -> anyhow::Result<()> {
        match self.mode {
            Mode::SelectFile => self.select_file.draw(frame, area),
            Mode::SelectHistory => self.select_history.draw(frame, area),
        }
    }
}
