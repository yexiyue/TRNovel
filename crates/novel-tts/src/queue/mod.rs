//! TTS队列模块，提供音频流队列功能
//!
//! 该模块实现了TTS文本转语音的队列处理机制，允许将多个音频片段按顺序排队播放，
//! 并支持在播放过程中动态添加新的音频片段。核心组件包括:
//! - [TTSQueueInput][]: 队列输入端，用于添加音频片段
//! - [TTSQueueOutput][]: 队列输出端，实现了rodio的Source trait，可直接用于播放
//! - [SoundOrSilence][]: 音频或静音枚举，用于处理队列中的空闲状态

mod queue_input;
mod queue_output;
mod sound_or_silence;
use std::sync::{Arc, Mutex, atomic::AtomicBool};

pub use queue_input::TTSQueueInput;
pub use queue_output::TTSQueueOutput;
use rodio::Source;
pub use sound_or_silence::SoundOrSilence;
use tokio::sync::mpsc::Sender;

/// 信号发送器类型别名，用于通知音频片段的播放状态
/// None表示无信号监听，Some(Sender)表示有监听者
type Signal = Option<Sender<bool>>;

/// 静音片段的采样阈值
/// 当队列中没有音频片段时，使用此长度的静音片段填充
const THRESHOLD: usize = 512;

/// 创建一个新的TTS队列
///
/// 返回一个元组，包含队列的输入端和输出端
///
/// # 泛型参数
/// * `T` - 音频源类型，必须实现Source、Send和Clone trait
///
/// # 返回值
/// * `Arc<TTSQueueInput<T>>` - 队列输入端的原子引用计数指针，可用于添加音频片段
/// * `TTSQueueOutput<T>` - 队列输出端，实现了Source trait，可用于播放
///
/// # 示例
/// ```
/// let (input, output) = queue::<MyAudioSource>();
/// input.append(audio_source);
/// // 使用output播放音频
/// ```
pub fn queue<T>() -> (Arc<TTSQueueInput<T>>, TTSQueueOutput<T>)
where
    T: Source + Send + Clone,
{
    // 创建输入端，包含音频片段向量和完成状态标志
    let input = Arc::new(TTSQueueInput {
        sounds: Mutex::new(Vec::new()),
        is_finished: AtomicBool::new(false),
    });

    // 创建输出端，初始索引为0
    let output = TTSQueueOutput::new(input.clone(), 0);

    (input, output)
}
