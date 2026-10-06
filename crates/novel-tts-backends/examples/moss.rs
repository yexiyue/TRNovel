//! Real CPU smoke test without opening an audio device.
use novel_tts_backends::moss::MossBackend;
use tts_core::backend::{AudioChunk, Backend};
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let directory = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("model directory required"))?;
    let output = args.next().unwrap_or_else(|| "moss.wav".into());
    let text = args
        .next()
        .unwrap_or_else(|| "你好，欢迎使用听书功能。".into());
    let text = if let Some(path) = text.strip_prefix('@') {
        std::fs::read_to_string(path)?
    } else {
        text
    };
    let voice = args.next().unwrap_or_else(|| "Weiguo".into());
    let device = match args.next().as_deref() {
        Some("coreml") => tts_protocol::Device::Coreml,
        Some("cuda") => tts_protocol::Device::Cuda,
        _ => tts_protocol::Device::Cpu,
    };
    let started = std::time::Instant::now();
    let backend = MossBackend::load_on(directory.into(), device).await?;
    eprintln!("loaded {:?}", started.elapsed());
    let start = std::time::Instant::now();
    let pieces = backend.segments(&text).await?;
    eprintln!("segments {}", pieces.len());
    let mut wav = hound::WavWriter::create(
        output,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )?;
    let mut count = 0;
    anyhow::ensure!(!pieces.is_empty(), "empty text");
    for piece in &pieces {
        let mut ended = false;
        let mut stream = backend.stream_seeded(&piece.text, &voice, Some(42)).await?;
        while let Some(chunk) = stream.recv().await {
            match chunk? {
                AudioChunk::Pcm(pcm) => {
                    if count == 0 {
                        eprintln!("first audio {:?}", start.elapsed());
                    }
                    count += pcm.samples.len();
                    for sample in pcm.samples {
                        wav.write_sample(sample)?;
                    }
                }
                AudioChunk::End => {
                    ended = true;
                    break;
                }
            }
        }
        anyhow::ensure!(ended && count > 0, "missing audio/completion");
    }
    wav.finalize()?;
    eprintln!(
        "audio {:.3}s, generation {:?}",
        count as f64 / 96000.,
        start.elapsed()
    );
    let segments = backend.segments(&text).await?;
    anyhow::ensure!(!segments.is_empty(), "missing source segments");
    Ok(())
}
