//! Exercise native audio preprocessing and Qwen timestamp decoding.
use std::sync::Arc;
use tts_core::{
    alignment::{Aligner, AudioClip, SpeechText},
    backend::Pcm,
};
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    eprintln!("ONNX Runtime: {:?}", ort::info());
    let mut args = std::env::args().skip(1);
    let model = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("model directory required"))?;
    let wav = args.next().ok_or_else(|| anyhow::anyhow!("WAV required"))?;
    let text = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("transcript required"))?;
    let mut reader = hound::WavReader::open(wav)?;
    let spec = reader.spec();
    anyhow::ensure!(
        spec.sample_format == hound::SampleFormat::Float,
        "probe expects float WAV"
    );
    let samples = reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?;
    let audio = AudioClip {
        blocks: vec![Arc::new(Pcm {
            samples,
            sample_rate: spec.sample_rate,
            channels: spec.channels,
        })],
        sample_rate: spec.sample_rate,
        channels: spec.channels,
        retention: Vec::new(),
    };
    let device = match args.next().as_deref() {
        Some("coreml") => tts_protocol::Device::Coreml,
        Some("cuda") => tts_protocol::Device::Cuda,
        _ => tts_protocol::Device::Cpu,
    };
    let aligner = novel_tts_backends::alignment::QwenAligner::load_on(model.into(), device).await?;
    let measurements = novel_tts_backends::devices::calibration::alignment(
        &*aligner,
        &SpeechText::from_source(&text, 0),
        &audio,
    )
    .await?;
    eprintln!("calibration {device:?}: {measurements:?}");
    let start = std::time::Instant::now();
    let sentences = aligner
        .align(&SpeechText::from_source(&text, 0), &audio)
        .await
        .map_err(|e| anyhow::anyhow!(e))?;
    for sentence in sentences {
        println!(
            "{}..{} {:.3}..{:.3} {}",
            sentence.range.start,
            sentence.range.end,
            sentence.start_frame as f64 / audio.sample_rate as f64,
            sentence.end_frame as f64 / audio.sample_rate as f64,
            &text[sentence.range.start..sentence.range.end]
        );
    }
    eprintln!("alignment {:?}", start.elapsed());
    Ok(())
}
