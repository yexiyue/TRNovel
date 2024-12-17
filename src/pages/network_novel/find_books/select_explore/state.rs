use ratatui::widgets::ListState;
use std::ops::{Deref, DerefMut};

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
