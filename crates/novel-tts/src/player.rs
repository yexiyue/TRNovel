use rodio::{Sink, mixer::Mixer, queue::SourcesQueueOutput};
use std::ops::{Deref, DerefMut};

/// TTS播放器结构体
///
/// 用于播放文本转语音的音频输出
/// 包装了Rodio的Sink，提供更方便的音频播放控制
pub struct Player {
    /// Rodio Sink用于控制音频播放
    pub sink: Sink,
}

impl Player {
    /// 创建新的播放器实例
    ///
    /// # 参数
    /// * `input` - 音频源队列输出
    /// * `mixer` - 音频混音器引用
    ///
    /// # 返回值
    /// 返回一个新的Player实例
    pub fn new(input: SourcesQueueOutput, mixer: &Mixer) -> Self {
        let sink = rodio::Sink::connect_new(mixer);
        sink.append(input);
        Self { sink }
    }
}

impl Deref for Player {
    type Target = Sink;

    /// 解引用到内部的Sink对象
    /// 允许直接调用Sink的方法
    fn deref(&self) -> &Self::Target {
        &self.sink
    }
}

impl DerefMut for Player {
    /// 可变解引用到内部的Sink对象
    /// 允许直接调用Sink的可变方法
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.sink
    }
}
