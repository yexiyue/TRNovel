//! Text-to-speech session core. Models and output devices stay with their owners;
//! consumers use model-independent PCM, controls and original-text byte events.
//!
//! Run sessions inside a Tokio `LocalSet`. Kokoro inference uses one dedicated
//! worker thread; only text and PCM cross its channel. The `kokoro` feature is
//! enabled by default. Reader integrations depend on `novel-tts-protocol` only.

pub mod backend;
pub mod checkpoint;
pub mod config;
pub mod download;
mod error;
pub mod models;
pub mod player;
pub mod session;
mod storage;
pub mod text;

pub use error::{ResourceError, Result};
pub use models::{CheckpointModel, VoicesData};
#[cfg(feature = "kokoro")]
pub use player::AudioPlayer;
pub use player::Playback;
