use super::{Component, LoadingPage};
use crate::{
    app::state::State,
    events::Events,
    file_list::NovelFiles,
    routes::{Route, Router},
};
use anyhow::Result;
use crossterm::event::KeyCode;
use ratatui::{
    layout::{Constraint, Layout},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Tabs},
};
use tokio::sync::mpsc::UnboundedSender;

use std::path::PathBuf;
use strum::{Display, EnumCount, EnumIter, FromRepr, IntoEnumIterator};

mod empty;
pub mod select_file;
pub mod select_history;
pub use select_file::SelectFile;
pub use select_history::SelectHistory;

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

    fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        _tx: UnboundedSender<Events>,
        _state: State,
    ) -> anyhow::Result<()> {
        match key.code {
            KeyCode::Tab => {
                self.mode = self.mode.toggle();
            }
            KeyCode::Char('i') => {
                self.show_info = !self.show_info;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_events(
        &mut self,
        events: crate::events::Events,
        tx: UnboundedSender<Events>,
        state: State,
    ) -> Result<()> {
        if let Events::KeyEvent(key) = events.clone() {
            self.handle_key_event(key, tx.clone(), state.clone())?
        }

        match self.mode {
            Mode::SelectFile => self.select_file.handle_events(events, tx, state)?,
            Mode::SelectHistory => self.select_history.handle_events(events, tx, state)?,
        }

        Ok(())
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

impl Router for LoadingPage<SelectNovel<'static>, PathBuf> {
    fn init(&mut self, tx: UnboundedSender<Events>, state: State) -> Result<()> {
        let path = self.args.to_path_buf();
        let inner = self.inner.clone();
        let history = state.history.clone();
        tokio::spawn(async move {
            match (|| {
                let novel_files = NovelFiles::from_path(path)?;

                match novel_files {
                    NovelFiles::File(path) => {
                        tx.send(Events::ReplaceRoute(Route::ReadNovel(path)))?;
                    }
                    NovelFiles::FileTree(tree) => {
                        let select_file = SelectFile::new(tree)?;
                        let select_history = SelectHistory::new(history);

                        *inner.try_lock()? = Some(SelectNovel::new(select_file, select_history)?);
                    }
                }
                Ok::<_, anyhow::Error>(())
            })() {
                Ok(_) => {}
                Err(e) => {
                    tx.send(Events::Error(e.to_string())).unwrap();
                }
            }
        });

        Ok(())
    }
}
