use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error(transparent)]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Reqwest(#[from] reqwest::Error),
    #[error(transparent)]
    Regex(#[from] regex::Error),
    #[error(transparent)]
    JsonPath(#[from] jsonpath_rust::JsonPathParserError),

    #[error(transparent)]
    ParseError(#[from] std::num::ParseIntError),
    #[error(transparent)]
    OtherError(#[from] anyhow::Error),
    #[error("{0}")]
    Warning(String),
}

pub type Result<T> = std::result::Result<T, ParseError>;
