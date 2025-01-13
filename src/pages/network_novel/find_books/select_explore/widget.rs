use parse_book_source::ExploreList;
use ratatui::{
    layout::{Constraint, Flex, Layout, Margin},
    style::Stylize,
    text::Line,
    widgets::{
        Block, Clear, List, Padding, Scrollbar, ScrollbarState, StatefulWidget, StatefulWidgetRef,
        Widget,
    },
};

use crate::THEME_CONFIG;

use super::state::SelectExploreState;

pub struct SelectExploreWidget<'a> {
    pub explore: ExploreList,
    pub list: List<'a>,
}

impl SelectExploreWidget<'_> {
    pub fn new(explore: ExploreList) -> Self {
        Self {
            list: List::new(
                explore
                    .iter()
                    .map(|x| Line::from(x.title.clone()).centered())
                    .collect::<Vec<_>>(),
            )
            .style(THEME_CONFIG.basic.text)
            .highlight_style(THEME_CONFIG.selected),
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

            let [horizontal] = Layout::horizontal([Constraint::Length(20)])
                .flex(Flex::Center)
                .areas(area);

            let [block_area] = Layout::vertical([Constraint::Max(12)])
                .flex(Flex::Center)
                .areas(horizontal);

            Clear.render(block_area, buf);

            let block_area = block_area.inner(Margin::new(2, 1));

            let block = Block::bordered()
                .title(
                    Line::from("频道")
                        .style(THEME_CONFIG.basic.border_title)
                        .centered(),
                )
                .border_style(THEME_CONFIG.basic.border)
                .padding(Padding::horizontal(1));

            self.list
                .render_ref(block.inner(block_area), buf, &mut state.state);

            block.render(block_area, buf);

            let mut scrollbar_state = ScrollbarState::new(self.explore.len())
                .position(state.state.selected().unwrap_or(0));

            Scrollbar::default().render(block_area, buf, &mut scrollbar_state);
        }
    }
}
