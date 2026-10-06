/// Playback boundary used by the session state machine without audio-library types.
pub trait Playback {
    fn append(&self, audio: std::sync::Arc<crate::backend::Pcm>);
    /// Monotonic position in original audio time; paused playback does not advance.
    fn position(&self) -> std::time::Duration;
    fn is_empty(&self) -> bool;
    fn pause(&self);
    fn resume(&self);
    fn stop(&self);
    fn configure(&self, volume: f32, speed: f32);
}

mod audio;
pub use audio::AudioPlayer;
