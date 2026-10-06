/// Playback boundary used by the session state machine without audio-library types.
pub trait Playback {
    fn append(&self, audio: crate::backend::Pcm);
    fn is_empty(&self) -> bool;
    fn pause(&self);
    fn resume(&self);
    fn stop(&self);
    fn configure(&self, volume: f32, speed: f32);
}

#[cfg(feature = "kokoro")]
mod audio;
#[cfg(feature = "kokoro")]
pub use audio::AudioPlayer;
