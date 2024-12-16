use std::sync::{Arc, Mutex};

use parse_book_source::JsonSource;
use ratatui::layout::Size;

use crate::history::History;

#[derive(Debug, Clone)]
pub struct State {
    pub history: Arc<Mutex<History>>,
    pub size: Arc<Mutex<Option<Size>>>,
    pub book_source: Arc<futures::lock::Mutex<Option<JsonSource>>>,
}
