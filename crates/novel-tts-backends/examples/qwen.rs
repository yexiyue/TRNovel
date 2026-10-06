//! Offline synthesis probe, using the same PCM stream as the worker.
use std::{path::PathBuf, time::Instant};
use tts_core::backend::{AudioChunk, Backend};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let directory = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("model directory required"))?,
    );
    let output = PathBuf::from(
        args.next()
            .ok_or_else(|| anyhow::anyhow!("output WAV required"))?,
    );
    let device = match args.next().as_deref().unwrap_or("cpu") {
        "cpu" => tts_protocol::Device::Cpu,
        "metal" => tts_protocol::Device::Metal,
        other => anyhow::bail!("unsupported probe device {other}"),
    };
    let text = args.next().unwrap_or_else(|| "你好，欢迎收听。".into());
    let started = Instant::now();
    let backend = novel_tts_backends::qwen::QwenBackend::load_on(directory, device).await?;
    let load_ms = started.elapsed().as_millis();
    let started = Instant::now();
    let mut stream = backend.stream(&text, "uncle_fu").await?;
    let mut samples = Vec::new();
    let mut first_ms = None;
    let mut eos = false;
    while let Some(chunk) = stream.recv().await {
        match chunk? {
            AudioChunk::Pcm(pcm) => {
                first_ms.get_or_insert_with(|| started.elapsed().as_millis());
                samples.extend(pcm.samples);
            }
            AudioChunk::End => {
                eos = true;
                break;
            }
        }
    }
    anyhow::ensure!(eos, "incomplete stream");
    let total_ms = started.elapsed().as_millis();
    let mut wav = hound::WavWriter::create(
        output,
        hound::WavSpec {
            channels: 1,
            sample_rate: 24000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )?;
    for sample in &samples {
        wav.write_sample(*sample)?;
    }
    wav.finalize()?;
    println!(
        "{}",
        serde_json::json!({"device":format!("{device:?}"),"load_ms":load_ms,
        "first_ms":first_ms,"total_ms":total_ms,"audio_seconds":samples.len() as f64 / 24000.0,"eos":eos})
    );
    Ok(())
}
