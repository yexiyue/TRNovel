//! Optional model-specific generation evidence; never used as speech coverage proof.
use serde::Serialize;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationEnd {
    Eos,
    FrameLimit,
    Cancelled,
    InferenceFailure,
}
#[derive(Debug, Default)]
pub(super) struct GenerationStats {
    pub frames: usize,
    pub audio_seconds: f64,
}
#[derive(Debug, Serialize)]
pub struct GenerationReport {
    pub normalized_text: String,
    pub token_count: usize,
    pub generated_frames: usize,
    pub audio_seconds: f64,
    pub end: GenerationEnd,
    pub error: Option<String>,
}
