//! 音频或静音枚举模块
//!
//! 该模块定义了SoundOrSilence枚举，用于表示音频片段或静音片段。
//! 这在TTS队列中非常有用，当队列为空时可以播放静音片段，避免播放中断。

use rodio::{Sample, Source, source::Zero};

/// 音频或静音枚举
///
/// 用于表示音频片段或静音片段，在TTS队列中使用。
/// 当队列中有音频片段时播放音频，当队列为空时播放静音。
///
/// # 泛型参数
/// * `T` - 音频源类型，必须实现Source trait
pub enum SoundOrSilence<T>
where
    T: Source,
{
    /// 音频片段
    Sound(T),

    /// 静音片段
    Silence(Zero),
}

impl<T> Iterator for SoundOrSilence<T>
where
    T: Source,
{
    type Item = Sample;

    /// 获取下一个音频采样
    ///
    /// 如果是音频片段，则返回音频采样；
    /// 如果是静音片段，则返回静音采样。
    fn next(&mut self) -> Option<Self::Item> {
        match self {
            // 如果是音频片段，则返回音频的下一个采样
            SoundOrSilence::Sound(source) => source.next(),
            // 如果是静音片段，则返回静音的下一个采样
            SoundOrSilence::Silence(silence) => silence.next(),
        }
    }

    /// 获取大小提示
    ///
    /// 返回迭代器剩余元素数量的估计值
    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            // 如果是音频片段，则返回音频的大小提示
            SoundOrSilence::Sound(source) => source.size_hint(),
            // 如果是静音片段，则返回静音的大小提示
            SoundOrSilence::Silence(silence) => silence.size_hint(),
        }
    }
}

impl<T> Source for SoundOrSilence<T>
where
    T: Source,
{
    /// 返回当前片段的跨度长度
    fn current_span_len(&self) -> Option<usize> {
        match self {
            // 如果是音频片段，则返回音频的跨度长度
            SoundOrSilence::Sound(source) => source.current_span_len(),
            // 如果是静音片段，则返回静音的跨度长度
            SoundOrSilence::Silence(silence) => silence.current_span_len(),
        }
    }

    /// 返回音频的声道数
    fn channels(&self) -> rodio::ChannelCount {
        match self {
            // 如果是音频片段，则返回音频的声道数
            SoundOrSilence::Sound(source) => source.channels(),
            // 如果是静音片段，则返回静音的声道数
            SoundOrSilence::Silence(silence) => silence.channels(),
        }
    }

    /// 返回音频的采样率
    fn sample_rate(&self) -> rodio::SampleRate {
        match self {
            // 如果是音频片段，则返回音频的采样率
            SoundOrSilence::Sound(source) => source.sample_rate(),
            // 如果是静音片段，则返回静音的采样率
            SoundOrSilence::Silence(silence) => silence.sample_rate(),
        }
    }

    /// 返回音频的总时长
    ///
    /// 音频片段和静音片段都没有固定的总时长，所以返回None
    fn total_duration(&self) -> Option<std::time::Duration> {
        match self {
            // 如果是音频片段，则返回音频的总时长
            SoundOrSilence::Sound(source) => source.total_duration(),
            // 如果是静音片段，则返回静音的总时长
            SoundOrSilence::Silence(silence) => silence.total_duration(),
        }
    }
}
