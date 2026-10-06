//! TTS模型和语音数据管理模块
//!
//! 该模块提供了对TTS所需模型文件和语音数据的管理功能，
//! 包括自动下载、路径管理和状态检查等。

use crate::download::{Download, get_cache_dir};
use std::{
    ops::{Deref, DerefMut},
    path::Path,
};

/// TTS模型文件的下载地址
static CHECKPOINT_URL: &str =
    "https://github.com/mzdk100/kokoro/releases/download/V1.1/kokoro-v1.1-zh.onnx";

/// 语音数据文件的下载地址
static VOICES_URL: &str =
    "https://github.com/mzdk100/kokoro/releases/download/V1.1/voices-v1.1-zh.bin";

/// TTS检查点模型管理器
///
/// 负责管理TTS引擎所需的检查点模型文件，包括下载、路径管理和状态检查。
/// 该结构体是对Download的封装，提供了更具体的模型文件管理功能。
#[derive(Debug, Clone)]
pub struct CheckpointModel(Download);

impl CheckpointModel {
    /// 创建新的检查点模型管理器
    ///
    /// # 参数
    /// * `path` - 模型文件的本地存储路径
    ///
    /// # 返回值
    /// 返回一个新的CheckpointModel实例
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self(Download::new(path, CHECKPOINT_URL))
    }
}

impl Deref for CheckpointModel {
    type Target = Download;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for CheckpointModel {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Default for CheckpointModel {
    /// 创建默认的检查点模型管理器
    ///
    /// 默认会将模型文件存储在用户主目录下的.novel-tts/kokoro目录中
    ///
    /// # 返回值
    /// 返回一个配置了默认路径和URL的CheckpointModel实例
    fn default() -> Self {
        let cache_dir = get_cache_dir().unwrap().join("kokoro");
        let path = cache_dir.join("kokoro-v1.1-zh.onnx");
        Self(Download::new(path, CHECKPOINT_URL))
    }
}

/// 语音数据管理器
///
/// 负责管理TTS引擎所需的语音数据文件，包括下载、路径管理和状态检查。
/// 该结构体是对Download的封装，提供了更具体的语音数据文件管理功能。
#[derive(Debug, Clone)]
pub struct VoicesData(Download);

impl VoicesData {
    /// 创建新的语音数据管理器
    ///
    /// # 参数
    /// * `path` - 语音数据文件的本地存储路径
    ///
    /// # 返回值
    /// 返回一个新的VoicesData实例
    pub fn new<P: AsRef<Path>>(path: P) -> Self {
        Self(Download::new(path, VOICES_URL))
    }
}

impl Default for VoicesData {
    /// 创建默认的语音数据管理器
    ///
    /// 默认会将语音数据文件存储在用户主目录下的.novel-tts/kokoro目录中
    ///
    /// # 返回值
    /// 返回一个配置了默认路径和URL的VoicesData实例
    fn default() -> Self {
        let cache_dir = get_cache_dir().unwrap().join("kokoro");
        let path = cache_dir.join("voices-v1.1-zh.bin");
        Self(Download::new(path, VOICES_URL))
    }
}

impl Deref for VoicesData {
    type Target = Download;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for VoicesData {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}
