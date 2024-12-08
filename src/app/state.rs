use std::sync::{Arc, Mutex};

use ratatui::layout::Size;

use crate::history::History;

#[derive(Debug, Clone)]
pub struct State {
    pub history: Arc<Mutex<History>>,
    pub size: Arc<Mutex<Option<Size>>>,
}
