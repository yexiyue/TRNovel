use kokoro_tts::Voice;
use novel_tts::{CheckpointModel, NovelTTS, VoicesData};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化模型和语音数据
    let model = CheckpointModel::default();
    let voices = VoicesData::default();

    // 检查并下载必要的模型文件
    if !model.is_downloaded() {
        model
            .async_download(|downloaded, total| {
                println!("模型下载进度: {}/{}", downloaded, total);
            })
            .await?;
    }

    if !voices.is_downloaded() {
        voices
            .async_download(|downloaded, total| {
                println!("语音数据下载进度: {}/{}", downloaded, total);
            })
            .await?;
    }

    // 创建TTS实例
    let novel_tts = NovelTTS::new(&model, &voices).await?;
    let mut chapter_tts = novel_tts.chapter_tts();

    // 准备要转换的文本
    let text = "这是小说的第一段落。\n这是第二段落。".to_string();

    // 流式处理文本到音频
    let (audio_queue, mut position_rx) = chapter_tts.stream(text, Voice::Zf006(1), |error| {
        eprintln!("TTS处理错误: {:?}", error)
    })?;

    // 监听字符位置更新
    tokio::spawn(async move {
        while let Some((start, end)) = position_rx.recv().await {
            println!("正在朗读字符位置: {} 到 {}", start, end);
        }
    });

    // 播放音频（需要rodio或其他音频播放库配合使用）
    let stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    let sink = rodio::Sink::connect_new(stream_handle.mixer());
    sink.append(audio_queue);
    sink.sleep_until_end();

    Ok(())
}
