use thiserror::Error;

#[derive(Debug, Error)]
pub enum Errors {
    #[error(transparent)]
    Anyhow(#[from] anyhow::Error),

    #[error("{0}")]
    Warning(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Lock(#[from] tokio::sync::TryLockError),

    #[error(transparent)]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    ParseBookSource(#[from] parse_book_source::ParseError),

    #[error(transparent)]
    Task(#[from] tokio::task::JoinError),
}

impl From<String> for Errors {
    fn from(value: String) -> Self {
        Self::Warning(value)
    }
}

impl From<&str> for Errors {
    fn from(value: &str) -> Self {
        Self::Warning(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Errors>;
