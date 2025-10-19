//! novel-tts库的错误类型定义

/// novel-tts库的错误类型枚举
///
/// 包含了库中可能出现的所有错误类型，通过thiserror派生Debug和Error trait
#[derive(thiserror::Error, Debug)]
pub enum NovelTTSError {
    /// Kokoro TTS引擎相关的错误
    #[error(transparent)]
    KokoroTtsError(#[from] kokoro_tts::KokoroError),

    /// IO操作相关的错误
    #[error(transparent)]
    IoError(#[from] std::io::Error),

    /// HTTP请求相关的错误
    #[error(transparent)]
    HttpError(#[from] reqwest::Error),

    /// 音频流处理相关的错误
    #[error(transparent)]
    Rodio(#[from] rodio::StreamError),

    /// 其他未分类的错误
    #[error(transparent)]
    OtherError(#[from] anyhow::Error),

    #[error("Cancelled: {0}")]
    Cancel(String),
}

/// novel-tts库的Result类型别名
///
/// 简化错误处理，所有库中的函数都应返回此类型
pub type Result<T> = std::result::Result<T, NovelTTSError>;
