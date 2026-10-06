//! Optional listening integration. The reader owns text and reading state;
//! native synthesis, playback and listening persistence belong to the child.
pub mod client;
pub mod controller;
pub use controller::{Handle, Snapshot};

#[derive(Clone)]
pub struct TtsContext {
    pub handle: Handle,
    pub snapshot: ratatui_kit::State<Snapshot>,
}
pub mod ui;
