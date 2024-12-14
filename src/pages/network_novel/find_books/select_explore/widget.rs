use parse_book_source::Explores;
use ratatui::{
    layout::{Constraint, Flex, Layout, Margin},
    style::{Style, Stylize},
    text::Line,
    widgets::{
        Block, Clear, List, Padding, Scrollbar, ScrollbarState, StatefulWidget, StatefulWidgetRef,
        Widget,
    },
};

use super::state::SelectExploreState;

pub struct SelectExploreWidget<'a> {
    pub explore: Explores,
    pub list: List<'a>,
}

impl SelectExploreWidget<'_> {
    pub fn new(explore: Explores) -> Self {
        Self {
            list: List::new(
                explore
                    .iter()
                    .map(|x| Line::from(x.title.clone()).centered())
                    .collect::<Vec<_>>(),
            )
            .highlight_style(Style::new().bold().on_light_cyan().black())
            .block(
                Block::bordered()
                    .title(Line::from("频道").centered())
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

            let [horizontal] = Layout::horizontal([Constraint::Length(20)])
                .flex(Flex::Center)
                .areas(area);

            let [block_area] = Layout::vertical([Constraint::Max(12)])
                .flex(Flex::Center)
                .areas(horizontal);

            Clear.render(block_area, buf);

            let block_area = block_area.inner(Margin::new(2, 1));

            self.list.render_ref(block_area, buf, &mut state.state);

            let mut scrollbar_state = ScrollbarState::new(self.explore.len())
                .position(state.state.selected().unwrap_or(0));

            Scrollbar::default().render(block_area, buf, &mut scrollbar_state);
        }
    }
}
