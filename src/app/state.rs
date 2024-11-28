use std::sync::{Arc, Mutex};

use crate::history::History;

#[derive(Debug, Clone)]
pub struct State {
    pub history: Arc<Mutex<History>>,
}
