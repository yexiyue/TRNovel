use crate::{book_source::BookSourceCache, history::History};
use ratatui::layout::Size;
use std::sync::Arc;
use tokio::sync::Mutex;

#[derive(Debug, Clone)]
pub struct State {
    pub history: Arc<Mutex<History>>,
    pub size: Arc<Mutex<Option<Size>>>,
    pub book_sources: Arc<Mutex<BookSourceCache>>,
}
