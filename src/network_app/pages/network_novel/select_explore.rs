use std::ops::{Deref, DerefMut};

use crate::errors::Result;
use crossterm::event::{KeyCode, KeyEventKind};
use parse_book_source::Explores;
use ratatui::{
    layout::{Constraint, Flex, Layout},
    style::{Style, Stylize},
    text::Line,
    widgets::{
        Block, Clear, List, ListState, Padding, Scrollbar, ScrollbarState, StatefulWidget,
        StatefulWidgetRef, Widget,
    },
};

use crate::components::Component;

#[derive(Debug, Default)]
pub struct SelectExploreState {
    pub state: ListState,
    pub show: bool,
}

impl SelectExploreState {
    pub fn toggle(&mut self) {
        self.show = !self.show;
    }
}

impl Deref for SelectExploreState {
    type Target = ListState;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for SelectExploreState {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

pub struct SelectExploreWidget<'a> {
    pub explore: Explores,
    pub list: List<'a>,
}

impl SelectExploreWidget<'_> {
    pub fn new(explore: Explores) -> Self {
        Self {
            list: List::new(explore.iter().map(|x| x.title.clone()).collect::<Vec<_>>())
                .highlight_style(Style::new().bold().on_light_cyan())
                .block(
                    Block::bordered()
                        .title(Line::from("目录").centered())
                        .border_style(Style::new().dim())
                        .padding(Padding::horizontal(1)),
                ),
            explore,
        }
    }
}

impl StatefulWidgetRef for SelectExploreWidget<'_> {
    type State = SelectExploreState;

    fn render_ref(
        &self,
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

            Clear.render(block_area, buf);

            self.list.render_ref(block_area, buf, &mut state.state);

            let mut scrollbar_state = ScrollbarState::new(self.explore.len())
                .position(state.state.selected().unwrap_or(0));

            Scrollbar::default().render(block_area, buf, &mut scrollbar_state);
        }
    }
}

pub struct SelectExplore<'a> {
    pub state: SelectExploreState,
    pub widget: SelectExploreWidget<'a>,
}

impl SelectExplore<'_> {
    pub fn new(explore: Explores) -> Self {
        Self {
            state: Default::default(),
            widget: SelectExploreWidget::new(explore),
        }
    }
}

impl Component for SelectExplore<'_> {
    fn draw(&mut self, frame: &mut ratatui::Frame, area: ratatui::prelude::Rect) -> Result<()> {
        self.widget
            .render_ref(area, frame.buffer_mut(), &mut self.state);
        Ok(())
    }

    fn handle_key_event(
        &mut self,
        key: crossterm::event::KeyEvent,
        _tx: tokio::sync::mpsc::UnboundedSender<crate::events::Events>,
        _state: crate::app::state::State,
    ) -> Result<()> {
        if key.kind != KeyEventKind::Press {
            return Ok(());
        }

        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                self.state.select_next();
            }
            KeyCode::Char('k') | KeyCode::Up => {
                self.state.select_previous();
            }
            KeyCode::Enter => {
                // self.state.toggle();
                todo!("发送事件，然后上一层接受该事件，然后加载loading，最后展示数据")
            }
            KeyCode::Tab => {
                self.state.toggle();
            }
            _ => {}
        }
        Ok(())
    }
}
