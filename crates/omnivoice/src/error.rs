use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum OmniVoiceError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("tokenizer error: {0}")]
    Tokenizer(#[from] tokenizers::Error),
    #[error("candle error: {0}")]
    Candle(#[from] candle_core::Error),
    #[error("wav error: {0}")]
    Hound(#[from] hound::Error),
    #[error("safetensors error: {0}")]
    SafeTensor(#[from] safetensors::SafeTensorError),
    #[error("missing artifact at {path}")]
    MissingArtifact { path: PathBuf },
    #[error("invalid tensor shape for {name}: expected {expected}, got {actual}")]
    InvalidTensorShape {
        name: String,
        expected: String,
        actual: String,
    },
    #[error("invalid request: {0}")]
    InvalidRequest(String),
    #[error("invalid data: {0}")]
    InvalidData(String),
    #[error("unsupported operation: {0}")]
    Unsupported(String),
}

pub type Result<T> = std::result::Result<T, OmniVoiceError>;
