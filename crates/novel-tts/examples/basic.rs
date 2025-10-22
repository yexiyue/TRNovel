use kokoro_tts::Voice;
use novel_tts::{CheckpointModel, NovelTTS, VoicesData};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化模型和语音数据
    let mut model = CheckpointModel::default();
    let mut voices = VoicesData::default();

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
    let text = "静夜思 李白
    床前明月光，
    疑似地上霜。
    举头望明月，
    低头思故乡。
"
    .to_string();

    let mut stream_handle = rodio::OutputStreamBuilder::open_default_stream()?;
    stream_handle.log_on_drop(false);
    let sink = rodio::Sink::connect_new(stream_handle.mixer());
    let mut chapter_tts = novel_tts.chapter_tts(&text);

    // 流式处理文本到音频
    let (audio_queue, mut position_rx) = chapter_tts.stream(Voice::Zf006(1), |error| {
        eprintln!("TTS处理错误: {:?}", error)
    });

    // 监听字符位置更新
    tokio::spawn(async move {
        let active_index = chapter_tts.texts;
        while let Some(index) = position_rx.recv().await {
            if let Some(index) = index {
                if let Some(chunk) = active_index.get(index) {
                    println!("当前播放文本: {}", chunk);
                }
            } else {
                println!("播放完成");
            }
        }
    });

    // 播放音频（需要rodio或其他音频播放库配合使用）

    sink.append(audio_queue);

    sink.sleep_until_end();

    Ok(())
}
