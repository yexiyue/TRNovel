//! TTS队列输入端模块
//!
//! 该模块定义了TTS队列的输入端，允许向队列中添加音频片段。
//! 输入端使用线程安全的结构，可以在多个线程中使用。

use rodio::Source;
use std::sync::{Mutex, atomic::AtomicBool};
use tokio::sync::mpsc::Receiver;

/// TTS队列输入端
///
/// 用于向TTS队列中添加音频片段。该结构是线程安全的，可以跨线程使用。
///
/// # 泛型参数
/// * `T` - 音频源类型，必须实现Source、Send和Clone trait
pub struct TTSQueueInput<T>
where
    T: Source + Send + Clone,
{
    /// 存储音频片段的向量，每个片段可以附带一个信号发送器
    /// 使用Mutex保证线程安全
    pub sounds: Mutex<Vec<(T, super::Signal)>>,

    /// 标记队列是否已完成（不再添加新的音频片段）
    /// 使用AtomicBool保证原子操作
    pub is_finished: AtomicBool,
}

impl<T> TTSQueueInput<T>
where
    T: Source + Send + Clone,
{
    /// 向队列末尾添加一个音频片段
    ///
    /// # 参数
    /// * `source` - 要添加的音频源
    pub fn append(&self, source: T) {
        // 锁定sounds向量并添加新的音频片段
        // 信号发送器为None，表示不关心播放状态
        self.sounds.lock().unwrap().push((source, None));
    }

    /// 向队列末尾添加一个带信号通知的音频片段
    ///
    /// 返回一个接收器，用于接收该音频片段的播放状态通知：
    /// - false: 音频片段开始播放
    /// - true: 音频片段播放完成
    ///
    /// # 参数
    /// * `source` - 要添加的音频源
    ///
    /// # 返回值
    /// * `Receiver<bool>` - 用于接收播放状态通知的接收器
    pub fn append_with_signal(&self, source: T) -> Receiver<bool> {
        // 创建一个容量为1的通道，用于发送播放状态信号
        let (tx, rx) = tokio::sync::mpsc::channel(1);

        // 锁定sounds向量并添加新的音频片段及信号发送器
        self.sounds.lock().unwrap().push((source, Some(tx)));

        rx
    }

    /// 设置队列完成状态
    ///
    /// 当设置为true时，表示不会再有新的音频片段添加到队列中
    ///
    /// # 参数
    /// * `is_finished` - 完成状态标志
    pub fn set_is_finished(&self, is_finished: bool) {
        // 使用Release内存序存储完成状态
        self.is_finished
            .store(is_finished, std::sync::atomic::Ordering::Release);
    }

    /// 检查队列是否已完成
    ///
    /// # 返回值
    /// * `bool` - 如果队列已完成返回true，否则返回false
    pub fn is_finished(&self) -> bool {
        // 使用Acquire内存序加载完成状态
        self.is_finished.load(std::sync::atomic::Ordering::Acquire)
    }
}
