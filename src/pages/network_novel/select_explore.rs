use std::ops::{Deref, DerefMut};

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
