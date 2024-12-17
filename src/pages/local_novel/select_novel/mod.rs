use crate::{
    app::State,
    components::{Component, KeyShortcutInfo, LoadingWrapperInit},
    file_list::NovelFiles,
    novel::local_novel::LocalNovel,
    pages::ReadNovel,
    Events, Navigator, Result, Router,
};
use async_trait::async_trait;
use crossterm::event::{KeyCode, KeyEvent, KeyEventKind};
use ratatui::{
    layout::{Constraint, Layout},
    style::{Style, Stylize},
    text::Line,
    widgets::{Block, Tabs},
};
use std::path::PathBuf;
use strum::{Display, EnumCount, EnumIter, FromRepr, IntoEnumIterator};

pub mod select_file;
pub mod select_history;
pub use select_file::SelectFile;
pub use select_history::SelectHistory;

#[derive(Debug)]
pub struct SelectNovel<'a> {
    pub select_file: SelectFile<'a>,
    pub select_history: SelectHistory,
    pub mode: Mode,
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

#[async_trait]
impl Component for SelectNovel<'_> {
    fn render(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        let [title, content_area] =
            Layout::vertical(vec![Constraint::Length(1), Constraint::Min(1)]).areas(area);
        self.render_tabs(frame, title);
        let block = Block::bordered();
        let inner_area = block.inner(content_area);
        frame.render_widget(block, content_area);
        self.render_tab(frame, inner_area)?;
        Ok(())
    }

    async fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        _state: State,
    ) -> Result<Option<KeyEvent>> {
        if key.kind != KeyEventKind::Press {
            return Ok(Some(key));
        }
        match key.code {
            KeyCode::Tab => {
                self.mode = self.mode.toggle();
                Ok(None)
            }
            _ => Ok(Some(key)),
        }
    }

    async fn handle_events(&mut self, events: Events, state: State) -> Result<Option<Events>> {
        let Some(events) = (match self.mode {
            Mode::SelectFile => {
                self.select_file
                    .handle_events(events, state.clone())
                    .await?
            }
            Mode::SelectHistory => {
                self.select_history
                    .handle_events(events, state.clone())
                    .await?
            }
        }) else {
            return Ok(None);
        };

        match events {
            Events::KeyEvent(key) => self
                .handle_key_event(key, state)
                .await
                .map(|item| item.map(Events::KeyEvent)),
            _ => Ok(Some(events)),
        }
    }

    fn key_shortcut_info(&self) -> KeyShortcutInfo {
        match self.mode {
            Mode::SelectFile => self.select_file.key_shortcut_info(),
            Mode::SelectHistory => self.select_history.key_shortcut_info(),
        }
    }
}

impl<'a> SelectNovel<'a> {
    pub fn new(select_file: SelectFile<'a>, select_history: SelectHistory) -> Result<Self> {
        Ok(Self {
            select_file,
            select_history,
            mode: Mode::default(),
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
    ) -> Result<()> {
        match self.mode {
            Mode::SelectFile => self.select_file.render(frame, area),
            Mode::SelectHistory => self.select_history.render(frame, area),
        }
    }
}

#[async_trait]
impl LoadingWrapperInit for SelectNovel<'static> {
    type Arg = PathBuf;
    async fn init(args: Self::Arg, navigator: Navigator, state: State) -> Result<Option<Self>> {
        let novel_files = NovelFiles::from_path(args)?;

        match novel_files {
            NovelFiles::File(path) => {
                let novel = LocalNovel::from_path(path)?;
                navigator.push(Box::new(ReadNovel::to_page_route(novel)))?;
                Ok(None)
            }
            NovelFiles::FileTree(tree) => {
                let select_file = SelectFile::new(tree, navigator.clone())?;
                let select_history = SelectHistory::new(state.history.clone(), navigator.clone());

                Ok(Some(SelectNovel::new(select_file, select_history)?))
            }
        }
    }
}

impl Router for SelectNovel<'_> {}
