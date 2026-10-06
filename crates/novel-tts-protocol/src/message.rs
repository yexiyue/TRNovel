use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A byte range in the exact UTF-8 text submitted by the client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextRange {
    pub start: usize,
    pub end: usize,
}

impl TextRange {
    /// Whether both endpoints form a valid half-open range in this snapshot.
    pub fn is_valid(self, text: &str) -> bool {
        self.start <= self.end
            && self.end <= text.len()
            && text.is_char_boundary(self.start)
            && text.is_char_boundary(self.end)
    }
}

/// Stable source identity; namespaces keep CLI files separate from reader books.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceId {
    pub namespace: String,
    pub book: String,
    pub chapter: String,
}

/// User preferences. `voice` retains the old JSON enum spelling, e.g. `Zf001`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub volume: f32,
    pub speed: f32,
    pub voice: String,
    pub auto_play: bool,
    pub backend: String,
    pub revision: u64,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            volume: 1.0,
            speed: 1.0,
            voice: "Zf001".into(),
            auto_play: false,
            backend: "kokoro".into(),
            revision: 0,
            extra: BTreeMap::new(),
        }
    }
}

/// Only changed fields are sent, so unrelated preferences can be preserved.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfigPatch {
    pub expected_revision: u64,
    pub volume: Option<f32>,
    pub speed: Option<f32>,
    pub voice: Option<String>,
    pub auto_play: Option<bool>,
}

/// A capability description; callers must not assume all backends are alike.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Capabilities {
    pub backend: String,
    pub voices: Vec<String>,
    pub native_streaming: bool,
    pub style: bool,
    pub cloning: bool,
    pub pronunciation: bool,
}

/// Validated immutable text and requested recovery position.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StartRequest {
    pub source: SourceId,
    pub text: String,
    pub text_hash: String,
    pub resume_byte: Option<usize>,
    pub restore_checkpoint: bool,
}

/// Commands sent by the parent. Session controls require an envelope session ID.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Command {
    Hello,
    GetStatus,
    GetConfig,
    UpdateConfig(ConfigPatch),
    PrepareModel,
    CancelPrepare,
    Start(StartRequest),
    Pause,
    Resume,
    Stop,
    Seek { byte: usize, new_session_id: String },
    Shutdown,
}

/// Command envelope with an opaque id for response correlation and deduplication.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub protocol_version: u32,
    pub request_id: String,
    pub session_id: Option<String>,
    #[serde(flatten)]
    pub command: Command,
}

/// User-observable state; generation and actual playback are distinct.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionState {
    Idle,
    Preparing,
    Generating,
    Playing,
    Paused,
    Stopped,
    Failed,
}

/// A terminal reason. Only Completed is eligible for automatic next chapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndReason {
    Completed,
    Cancelled,
    Failed,
}

/// Structured errors stay actionable without requiring stderr parsing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrorInfo {
    pub code: String,
    pub stage: String,
    pub message: String,
    pub retryable: bool,
}

/// Messages sent by the worker. Accepted does not mean synthesis has completed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload", rename_all = "snake_case")]
pub enum Event {
    Ready(Capabilities),
    Accepted,
    Config(Config),
    ConfigChanged(Config),
    ModelProgress {
        resource: String,
        downloaded: u64,
        total: u64,
    },
    ModelReady,
    SessionState {
        state: SessionState,
    },
    SegmentStarted {
        range: TextRange,
        text_hash: String,
    },
    SegmentFinished {
        range: TextRange,
        text_hash: String,
    },
    SessionEnded {
        reason: EndReason,
        text_hash: String,
    },
    Error(ErrorInfo),
}

/// Response or async event; only responses have request_id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub protocol_version: u32,
    pub instance_id: String,
    pub session_id: Option<String>,
    pub sequence: u64,
    pub request_id: Option<String>,
    #[serde(flatten)]
    pub event: Event,
}
