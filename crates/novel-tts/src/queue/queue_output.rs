//! TTS队列输出端模块
//!
//! 该模块定义了TTS队列的输出端，实现了rodio的Source trait，可以直接用于音频播放。
//! 输出端负责按顺序播放队列中的音频片段，并在没有音频片段时播放静音。

use super::{Signal, SoundOrSilence, THRESHOLD, TTSQueueInput};
use anyhow::{Result, anyhow};
use rodio::{Sample, Source, source::Zero};
use std::sync::Arc;

/// TTS队列输出端
///
/// 实现了Iterator和Source trait，可以作为音频源直接使用。
/// 自动处理队列中音频片段的顺序播放。
///
/// # 泛型参数
/// * `T` - 音频源类型，必须实现Source、Send和Clone trait
pub struct TTSQueueOutput<T>
where
    T: Source + Send + Clone,
{
    /// 当前正在播放的音频或静音片段
    pub current: SoundOrSilence<T>,

    /// 当前音频片段关联的信号发送器
    /// 用于通知播放状态（开始/结束）
    pub current_signal: Signal,

    /// 当前播放片段在队列中的索引
    pub index: usize,

    /// 队列输入端的引用
    pub input: Arc<TTSQueueInput<T>>,

    /// 是否为初始状态
    /// 初始状态下会播放索引为index的片段，而非index+1的片段
    pub is_initial: bool,
}

impl<T> TTSQueueOutput<T>
where
    T: Source + Send + Clone,
{
    /// 创建一个新的队列输出端
    ///
    /// # 参数
    /// * `input` - 队列输入端的引用
    /// * `index` - 初始播放索引
    ///
    /// # 返回值
    /// * `TTSQueueOutput<T>` - 新创建的队列输出端
    pub fn new(input: Arc<TTSQueueInput<T>>, index: usize) -> Self {
        // 尝试获取指定索引的音频片段
        let (sound, signal, is_initial) =
            if let Some((sound, signal)) = input.sounds.lock().unwrap().get(index) {
                // 如果存在该索引的音频片段，则使用该片段
                (SoundOrSilence::Sound(sound.clone()), signal.clone(), false)
            } else {
                // 如果不存在该索引的音频片段，则创建一个静音片段
                let silence = Zero::new_samples(1, 44100, THRESHOLD);
                (SoundOrSilence::Silence(silence), None, true)
            };

        Self {
            current: sound,
            current_signal: signal,
            index,
            input,
            is_initial,
        }
    }

    /// 切换到下一个音频片段
    ///
    /// 如果队列中还有下一个音频片段，则切换到该片段；
    /// 如果队列已完成且没有更多片段，则返回错误；
    /// 如果队列未完成但没有更多片段，则播放静音片段。
    ///
    /// # 返回值
    /// * `Result<()>` - 成功返回Ok，如果队列已完成且没有更多片段则返回Err
    pub fn go_next(&mut self) -> Result<()> {
        // 如果当前片段有关联的信号发送器，则发送结束信号
        if let Some(sender) = self.current_signal.take() {
            // 发送上一个结束信号
            sender.try_send(true).ok();
        }

        // 计算下一个要播放的片段索引
        let next_index = if self.is_initial {
            // 如果是初始状态，则播放当前索引的片段
            self.index
        } else {
            // 否则播放下一个索引的片段
            self.index + 1
        };

        // 尝试获取下一个音频片段
        if let Some((sound, signal)) = self.input.sounds.lock().unwrap().get(next_index) {
            // 如果存在下一个音频片段，则切换到该片段
            self.current = SoundOrSilence::Sound(sound.clone());
            self.current_signal = signal.clone();
            self.index = next_index;
            self.is_initial = false;
        } else {
            // 如果不存在下一个音频片段
            if self.input.is_finished() {
                // 如果队列已完成，则返回错误
                return Err(anyhow!("No more sounds in the queue"));
            }
            // 如果队列未完成，则播放静音片段
            let silence = Zero::new_samples(1, 44100, THRESHOLD);
            self.current = SoundOrSilence::Silence(silence);
        }

        // 如果新片段有关联的信号发送器，则发送开始信号
        if let Some(sender) = &self.current_signal {
            // 发送新的开始信号
            sender.try_send(false).ok();
        }

        Ok(())
    }
}

impl<T> Iterator for TTSQueueOutput<T>
where
    T: Source + Send + Clone,
{
    type Item = Sample;

    fn next(&mut self) -> Option<Self::Item> {
        // 循环处理音频片段
        loop {
            // 尝试从当前音频片段获取下一个采样
            if let Some(sample) = self.current.next() {
                return Some(sample);
            }

            // 如果当前音频片段已播放完毕，则切换到下一个片段
            if self.go_next().is_err() {
                // 如果切换失败（队列已完成且没有更多片段），则返回None
                return None;
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        // 返回当前音频片段的大小提示
        (self.current.size_hint().0, None)
    }
}

impl<T> Source for TTSQueueOutput<T>
where
    T: Source + Send + Clone,
{
    /// 返回当前片段的跨度长度
    ///
    /// 该函数确定当前音频片段的跨度长度，用于音频处理中的缓冲区管理。
    /// 队列中两个音频片段之间的边界也应作为跨度边界处理。
    ///
    /// 确定跨度长度的优先级：
    /// 1. 当前音频片段提供的跨度长度（非零值）
    /// 2. 当队列为空且未完成时，使用静音片段长度
    /// 3. 使用音频片段的大小提示（正值）
    /// 4. 默认使用THRESHOLD常量
    ///
    /// # 返回值
    /// * `Option<usize>` - 当前片段的跨度长度
    fn current_span_len(&self) -> Option<usize> {
        // 尝试获取当前片段的跨度长度
        if let Some(val) = self.current.current_span_len() {
            if val != 0 {
                return Some(val);
            } else if !self.input.is_finished() && self.input.sounds.lock().unwrap().is_empty() {
                // 下一个片段将是填充的静音片段，其长度为THRESHOLD
                return Some(THRESHOLD);
            }
        }

        // 尝试获取大小提示
        let (lower_bound, _) = self.current.size_hint();
        // 迭代器的默认实现返回0，这是一个有问题的值，所以跳过它
        if lower_bound > 0 {
            return Some(lower_bound);
        }

        // 否则使用常量值
        Some(THRESHOLD)
    }

    /// 返回音频的声道数
    fn channels(&self) -> rodio::ChannelCount {
        self.current.channels()
    }

    /// 返回音频的采样率
    fn sample_rate(&self) -> rodio::SampleRate {
        self.current.sample_rate()
    }

    /// 返回音频的总时长（无）
    fn total_duration(&self) -> Option<std::time::Duration> {
        None
    }
}
