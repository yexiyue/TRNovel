//! 章节TTS处理模块
//!
//! 该模块提供了针对小说章节的文本转语音功能，支持流式处理和实时字符位置追踪。
//!
//! # 功能特点
//! * 流式音频生成：支持边生成边播放，减少等待时间
//! * 字符位置追踪：精确记录每个音频片段对应的原文本位置
//! * 异步处理：使用Tokio异步运行时，保证主线程不被阻塞
//! * 可取消操作：支持中途取消TTS处理任务

use crate::{NovelTTSError, utils::preprocess_text};
use kokoro_tts::{KokoroTts, Voice};
use rodio::{
    buffer::SamplesBuffer,
    queue::{self, SourcesQueueOutput},
};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc::Receiver};
use tokio_util::sync::CancellationToken;

/// TTS章节处理器，负责将文本转换为音频并管理播放队列
#[derive(Clone)]
pub struct ChapterTTS {
    pub texts: Vec<String>,
    /// 音频缓冲区集合
    pub segments: Arc<Mutex<Vec<SamplesBuffer>>>,
    /// 取消令牌，用于取消TTS处理
    pub cancel_token: CancellationToken,
    pub active_index: Arc<Mutex<usize>>,
    pub tts: Arc<KokoroTts>,
}

impl ChapterTTS {
    /// 创建新的TTS章节处理器
    ///
    /// # 参数
    /// * `tts` - TTS引擎实例
    ///
    /// # 返回值
    /// 返回一个新的ChapterTTS实例
    pub fn new(tts: Arc<KokoroTts>, text: &str) -> Self {
        Self {
            segments: Arc::new(Mutex::new(Vec::new())),
            cancel_token: CancellationToken::new(),
            active_index: Arc::new(Mutex::new(0)),
            tts,
            texts: preprocess_text(text, 200),
        }
    }

    /// 流式处理文本并生成音频
    ///
    /// 将输入的文本按行分割，逐行转换为音频，并提供字符位置追踪功能。
    ///
    /// # 参数
    /// * `text` - 要转换的文本
    /// * `voice` - 使用的语音
    /// * `on_error` - 错误处理回调
    ///
    /// # 返回值
    /// 返回元组，包含音频队列输出和字符位置接收器
    ///
    /// # 注意事项
    /// * 音频是流式生成的，可以边生成边播放
    /// * 字符位置通过Receiver通道实时返回
    /// * 如果需要取消处理，可以调用cancel方法
    pub fn stream(
        &mut self,
        voice: Voice,
        on_error: impl Fn(NovelTTSError) + Send + Sync + 'static,
    ) -> (SourcesQueueOutput, Receiver<usize>) {
        let (audio_queue_tx, audio_queue_rx) = queue::queue(true);
        let (position_tx, position_rx) = tokio::sync::mpsc::channel::<usize>(1);

        self.cancel_token = CancellationToken::new();

        let cancel_token = self.cancel_token.clone();
        let chunks = self.segments.clone();
        let tts = self.tts.clone();
        let active_index = self.active_index.clone();
        let texts = self.texts.clone();

        tokio::spawn(async move {
            chunks.lock().await.clear();
            let n = *active_index.lock().await;
            for (index, chunk_text) in texts.iter().skip(n).enumerate() {
                tokio::select! {
                    _ = cancel_token.cancelled() => {
                        break;
                    }
                    res = tts.synth(chunk_text, voice) =>{
                        let Ok((data, _)) = res else{
                            on_error(NovelTTSError::from(res.err().unwrap()));
                            continue;
                        };
                        let buffer = SamplesBuffer::new(1, 24000, data);

                        let signal = audio_queue_tx.append_with_signal(buffer.clone());

                        chunks.lock().await.push(buffer);

                        if index == 0 {
                            let _ = position_tx.send(*active_index.lock().await).await;
                        }

                        tokio::spawn({
                            let position_tx = position_tx.clone();
                            let active_index = active_index.clone();
                            async move {
                                loop {
                                    if signal.recv().is_ok(){
                                        break;
                                    }
                                }
                                let new_index = *active_index.lock().await + 1;
                                *active_index.lock().await= new_index;
                                let _ = position_tx.send(new_index).await;
                            }
                        });
                    }
                }
            }
        });
        (audio_queue_rx, position_rx)
    }

    /// 取消当前的TTS处理
    ///
    /// 调用此方法会取消正在进行的TTS处理任务
    pub fn cancel(&self) {
        self.cancel_token.cancel();
    }
}
