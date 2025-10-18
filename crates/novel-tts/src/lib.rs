//! novel-tts 是一个专门为小说阅读设计的文本转语音（TTS）库。
//! 它基于 [kokoro-tts](https://github.com/mzdk100/kokoro) 引擎，
//! 提供了针对长文本（如小说章节）优化的流式处理功能。
//!
//! ## 功能特性
//!
//! - 📚 专为小说阅读优化的TTS解决方案
//! - 🌊 流式音频处理，支持边生成边播放
//! - 🎵 支持多种语音选择
//! - 🔁 实时字符位置追踪，便于同步文本高亮
//! - ⏹️ 支持播放控制（暂停、取消）
//! - 📦 自动下载和管理TTS模型文件
//! - 🧵 异步API设计，适用于现代Rust应用
//!
//! ## 快速开始
//!
//! ```rust
//! use novel_tts::{NovelTTS, CheckpointModel, VoicesData, ChapterTTS};
//! use kokoro_tts::Voice;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // 初始化模型和语音数据
//! let model = CheckpointModel::default();
//! let voices = VoicesData::default();
//!
//! // 检查并下载必要的模型文件
//! if !model.is_downloaded() {
//!     model.async_download(|downloaded, total| {
//!         println!("模型下载进度: {}/{}", downloaded, total);
//!     }).await?;
//! }
//!
//! if !voices.is_downloaded() {
//!     voices.async_download(|downloaded, total| {
//!         println!("语音数据下载进度: {}/{}", downloaded, total);
//!     }).await?;
//! }
//!
//! // 创建TTS实例
//! let novel_tts = NovelTTS::new(&model, &voices).await?;
//! let mut chapter_tts = novel_tts.chapter_tts();
//!
//! // 准备要转换的文本
//! let text = "这是小说的第一段落。\n这是第二段落。".to_string();
//!
//! // 流式处理文本到音频
//! let (audio_queue, mut position_rx) = chapter_tts.stream(
//!     text,
//!     Voice::Zf006(1),
//!     |error| eprintln!("TTS处理错误: {:?}", error)
//! )?;
//!
//! // 监听字符位置更新
//! tokio::spawn(async move {
//!     while let Some((start, end)) = position_rx.recv().await {
//!         println!("正在朗读字符位置: {} 到 {}", start, end);
//!     }
//! });
//! # Ok(())
//! # }
//! ```

mod chapter;
mod download;
mod error;
mod model;

// 重新导出公共类型
pub use chapter::*;
pub use error::*;
use kokoro_tts::KokoroTts;
pub use model::*;

use std::sync::Arc;

/// NovelTTS主结构体
///
/// 负责管理TTS引擎实例，是整个TTS功能的核心入口点。
pub struct NovelTTS {
    tts: Arc<KokoroTts>,
}

impl NovelTTS {
    /// 创建新的NovelTTS实例
    ///
    /// # 参数
    /// * `model` - TTS模型文件信息
    /// * `voices` - 语音数据文件信息
    ///
    /// # 返回值
    /// 返回Result包装的NovelTTS实例，如果模型或语音数据加载失败则返回错误
    pub async fn new(model: &CheckpointModel, voices: &VoicesData) -> Result<Self> {
        let tts = KokoroTts::new(&model.path, &voices.path).await?;
        Ok(Self { tts: Arc::new(tts) })
    }

    /// 创建章节TTS处理器
    ///
    /// # 返回值
    /// 返回一个新的ChapterTTS实例，用于处理特定章节的文本转语音
    pub fn chapter_tts(&self) -> ChapterTTS {
        ChapterTTS::new(self.tts.clone())
    }
}
