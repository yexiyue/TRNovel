use anyhow::Result;
use futures::stream::StreamExt;
use kokoro_tts::{KokoroTts, SynthStream, Voice};
use rodio::{OutputStreamBuilder, Sink, buffer::SamplesBuffer};

pub struct ChapterTTS {
    pub tts: KokoroTts,
    pub stream: SynthStream,
    pub speed: f32, // 添加播放速度属性，默认为1.0
}

impl ChapterTTS {
    pub async fn new(content: &str) -> Result<Self> {
        let tts = KokoroTts::new(
            "./test-novels/kokoro/kokoro-v1.1-zh.onnx",
            "./test-novels/kokoro/voices-v1.1-zh.bin",
        )
        .await?;

        let (mut sink, stream) = tts.stream(Voice::Zf006(1));

        for line in content.lines() {
            sink.synth(line.to_string()).await?;
        }

        // 默认播放速度为1.0（正常速度）
        Ok(Self {
            tts,
            stream,
            speed: 1.0,
        })
    }

    // 添加设置播放速度的方法
    pub fn set_speed(&mut self, speed: f32) {
        self.speed = speed;
    }

    pub async fn play(&mut self) -> Result<()> {
        let stream_handle = OutputStreamBuilder::open_default_stream()?;
        let sink = Sink::connect_new(stream_handle.mixer());

        // 设置播放速度
        sink.set_speed(self.speed);

        while let Some((audio, took)) = self.stream.next().await {
            eprintln!("Synth took: {:?}", took);
            sink.append(SamplesBuffer::new(1, 24000, audio));
        }
        sink.sleep_until_end();
        Ok(())
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    #[tokio::test]
    async fn test_tts() {
        let content = "时间为";
        let mut tts = ChapterTTS::new(content).await.unwrap();

        tts.set_speed(1.0);
        tts.play().await.unwrap();
    }
}
