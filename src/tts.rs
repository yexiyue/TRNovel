//! Optional listening integration. The reader owns text and reading state;
//! native synthesis, playback and listening persistence belong to the child.
pub mod cli;
pub mod client;
pub mod controller;
pub use controller::{Handle, Snapshot};

#[derive(Clone)]
pub struct TtsContext {
    pub handle: Handle,
    pub snapshot: ratatui_kit::State<Snapshot>,
}
pub mod ui;

/// Exact reader text, independent of the worker's synthesis configuration.
#[derive(Debug, Clone)]
pub struct ChapterRequest {
    pub source: tts_protocol::SourceId,
    pub text: String,
    pub text_hash: String,
    pub resume_byte: Option<usize>,
    pub restore_checkpoint: bool,
}

impl ChapterRequest {
    fn plan(
        &self,
        sound: &crate::config::SoundConfig,
        playback: crate::config::PlaybackPreference,
    ) -> tts_protocol::PlanRequest {
        tts_protocol::PlanRequest {
            source: self.source.clone(),
            text: self.text.clone(),
            text_hash: self.text_hash.clone(),
            backend: sound.backend.clone(),
            model: sound.model.clone(),
            voices: vec![sound.voice.clone()],
            spans: vec![tts_protocol::VoiceSpan {
                range: tts_protocol::TextRange {
                    start: 0,
                    end: self.text.len(),
                },
                voice: sound.voice.clone(),
                style: sound.style.clone(),
            }],
            sealed: true,
            playback: match playback {
                crate::config::PlaybackPreference::Streaming => {
                    tts_protocol::PlanPlayback::Streaming
                }
                crate::config::PlaybackPreference::AfterChapterReady => {
                    tts_protocol::PlanPlayback::AfterChapterReady
                }
            },
            resume_byte: self.resume_byte,
            restore_checkpoint: self.restore_checkpoint,
            continuation: true,
            // Novel paragraphs sit on blank lines, which the worker would
            // otherwise treat as chain resets between every paragraph.
            continuation_boundary: tts_protocol::ContinuationBoundary::Scene,
            // Chain-length budget stays on the worker default; it bounds
            // self-chained timbre drift, not text layout.
            continuation_span: None,
            params: Default::default(),
            // A chapter-stable seed keeps segment sampling from re-rolling
            // its register at every paragraph while replays stay identical.
            seed: Some(chapter_seed(&self.text_hash)),
            verification: Default::default(),
        }
    }
}

/// Deterministic per-chapter sampling seed derived from the chapter digest.
/// Any stable hash works; FNV-1a keeps it independent of std hasher details.
fn chapter_seed(hash: &str) -> u64 {
    let mut seed = 0xcbf2_9ce4_8422_2325_u64;
    for byte in hash.as_bytes() {
        seed ^= u64::from(*byte);
        seed = seed.wrapping_mul(0x0000_0100_0000_01b3);
    }
    seed
}

#[cfg(test)]
mod tests {
    use super::*;
    use tts_protocol::{ContinuationBoundary, SourceId, text_hash};

    #[test]
    fn plan_pins_a_chapter_seed_and_keeps_the_chain_across_paragraphs() {
        let sound = crate::config::SoundConfig {
            backend: "moss".into(),
            model: Some("nano".into()),
            voice: "Weiguo".into(),
            style: None,
            device: tts_protocol::Device::Auto,
        };
        let request = ChapterRequest {
            source: SourceId {
                namespace: "reader".into(),
                book: "b".into(),
                chapter: "1".into(),
            },
            text: "甲。\n\n乙。".into(),
            text_hash: text_hash("甲。\n\n乙。"),
            resume_byte: None,
            restore_checkpoint: false,
        };
        let plan = request.plan(&sound, crate::config::PlaybackPreference::default());
        assert!(plan.continuation);
        assert_eq!(plan.continuation_boundary, ContinuationBoundary::Scene);
        assert_eq!(plan.seed, Some(chapter_seed(&request.text_hash)));
        assert_eq!(chapter_seed("chapter-a"), chapter_seed("chapter-a"));
        assert_ne!(chapter_seed("chapter-a"), chapter_seed("chapter-b"));
        assert_eq!(plan.playback, tts_protocol::PlanPlayback::Streaming);
        assert_eq!(
            request
                .plan(&sound, crate::config::PlaybackPreference::AfterChapterReady)
                .playback,
            tts_protocol::PlanPlayback::AfterChapterReady
        );
    }
}
