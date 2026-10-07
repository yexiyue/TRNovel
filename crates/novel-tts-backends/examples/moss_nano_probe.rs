//! Seeded ONNX/Candle comparison through the exact streaming adapters.
use novel_tts_backends::moss::{MossBackend, nano::NanoBackend};
use std::{path::PathBuf, time::Instant};
use tts_core::backend::{AudioChunk, Backend};
use tts_protocol::Device;
enum Adapter {
    Onnx(MossBackend),
    Candle(NanoBackend),
}
impl Adapter {
    async fn stream(
        &self,
        text: &str,
        voice: &str,
    ) -> anyhow::Result<tts_core::backend::AudioStream> {
        Ok(match self {
            Self::Onnx(model) => model.stream_seeded(text, voice, Some(42)).await?,
            Self::Candle(model) => model.stream_seeded(text, voice, 42).await?,
        })
    }
    fn backend(&self) -> &dyn Backend {
        match self {
            Self::Onnx(model) => model,
            Self::Candle(model) => model,
        }
    }
}
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args: Vec<_> = std::env::args().collect();
    anyhow::ensure!(
        args.len() >= 7,
        "onnx|candle root output.wav cpu|metal text voice"
    );
    let root = PathBuf::from(&args[2]);
    let device = match args[4].as_str() {
        "cpu" => Device::Cpu,
        "metal" => Device::Metal,
        _ => anyhow::bail!("invalid probe device"),
    };
    let started = Instant::now();
    let model = match args[1].as_str() {
        "onnx" => Adapter::Onnx(MossBackend::load_on(root.join("moss"), device).await?),
        "candle" => Adapter::Candle(NanoBackend::load_on(root, device).await?),
        _ => anyhow::bail!("invalid probe backend"),
    };
    let load_ms = started.elapsed().as_millis();
    if std::env::var_os("NOVEL_TTS_PROBE_CANCEL").is_some() {
        let mut stream = model.stream(&args[5], &args[6]).await?;
        anyhow::ensure!(
            matches!(stream.recv().await.transpose()?, Some(AudioChunk::Pcm(_))),
            "no PCM before cancellation"
        );
        drop(stream);
    }
    let started = Instant::now();
    let mut first = None;
    let mut samples = Vec::new();
    let segments = model.backend().segments(&args[5]).await?;
    anyhow::ensure!(!segments.is_empty(), "empty input");
    for segment in &segments {
        let mut stream = model.stream(&segment.text, &args[6]).await?;
        let mut ended = false;
        while let Some(chunk) = stream.recv().await {
            match chunk? {
                AudioChunk::Pcm(pcm) => {
                    pcm.duration_ms()?;
                    anyhow::ensure!(
                        (pcm.sample_rate, pcm.channels) == (48000, 2),
                        "invalid Nano format"
                    );
                    first.get_or_insert_with(|| started.elapsed().as_millis());
                    samples.extend(pcm.samples);
                }
                AudioChunk::End => {
                    ended = true;
                    break;
                }
            }
        }
        anyhow::ensure!(ended, "Nano stream did not reach EOS");
    }
    let generate_ms = started.elapsed().as_millis();
    let duration = samples.len() as f64 / 96000.;
    anyhow::ensure!(duration > 0., "empty Nano output");
    let mut wav = hound::WavWriter::create(
        &args[3],
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
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
        serde_json::json!({"backend":args[1],"device":args[4],"seed":42,"load_ms":load_ms,"first_pcm_ms":first,"generate_ms":generate_ms,"audio_seconds":duration,"rtf":generate_ms as f64/1000./duration,"eos":true})
    );
    if std::env::var_os("NOVEL_TTS_PROBE_DROP_EARLY").is_some() {
        let stream = model.stream(&args[5], &args[6]).await?;
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        let started = Instant::now();
        drop(model);
        drop(stream);
        println!(
            "{}",
            serde_json::json!({"owner_drop_with_live_stream_ms":started.elapsed().as_millis()})
        );
    }
    Ok(())
}
