//! Text-to-speech session core. Models and output devices stay with their owners;
//! consumers use model-independent PCM, controls and original-text byte events.
//!
//! Run sessions inside a Tokio `LocalSet`; concrete models live in novel-tts-backends.

pub mod alignment;
pub mod audio;
pub mod backend;
pub mod checkpoint;
pub mod config;
pub mod download;
mod error;
pub mod player;
pub mod session;
mod storage;
pub mod text;
pub mod voices;

pub use error::{ResourceError, Result};
pub use player::AudioPlayer;
pub use player::Playback;
