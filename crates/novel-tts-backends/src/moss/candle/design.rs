//! Design once with VoiceGenerator; narration reuses the saved WAV in Local/Realtime.
use super::{resources, runtime};
use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};
pub async fn reference(
    root: PathBuf,
    selected: tts_protocol::Device,
    text: String,
    description: String,
    output: PathBuf,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        selected == tts_protocol::Device::Cuda,
        "MOSS voice design currently requires verified CUDA/BF16"
    );
    anyhow::ensure!(
        !text.trim().is_empty() && text.chars().count() <= 60,
        "design reference must contain 1..60 characters"
    );
    anyhow::ensure!(
        !description.trim().is_empty() && description.len() <= 4096,
        "provide a voice description up to 4096 bytes"
    );
    let cancelled = Arc::new(AtomicBool::new(false));
    let signal = cancelled.clone();
    let (reply, result) = tokio::sync::oneshot::channel();
    let handle = std::thread::Builder::new()
        .name("moss-voice-design".into())
        .spawn(move || {
            let generated = (|| -> anyhow::Result<()> {
                let device = runtime::device(selected)?;
                let mut model = moss_tts::Model::load(
                    "voice-design-1.7b",
                    &resources::design_directory(&root),
                    &device,
                    runtime::model_dtype(&device),
                )?;
                let check = || signal.load(Ordering::Relaxed) || reply.is_closed();
                anyhow::ensure!(!check(), "voice design cancelled");
                let mut codec = moss_tts::codec::AudioCodec::load(
                    &resources::codec_directory(&root),
                    &device,
                    candle_core::DType::F16,
                )?;
                let mut request = moss_tts::Generation::new(&text);
                request.instruction = Some(&description);
                request.max_frames = 125;
                let mut frames = Vec::new();
                let mut samples = Vec::new();
                model.generate(&request, &check, |frame| {
                    frames.push(frame.to_vec());
                    if frames.len() == 5 {
                        samples.extend(codec.decode(&frames, &check)?);
                        frames.clear();
                    }
                    Ok(!check())
                })?;
                if !frames.is_empty() {
                    samples.extend(codec.decode(&frames, &check)?);
                }
                anyhow::ensure!(
                    !check() && !samples.is_empty(),
                    "voice design cancelled or empty"
                );
                let pcm = tts_core::backend::Pcm {
                    samples,
                    sample_rate: 24000,
                    channels: 1,
                };
                pcm.duration_ms()?;
                let mut wav = hound::WavWriter::create(
                    output,
                    hound::WavSpec {
                        channels: 1,
                        sample_rate: 24000,
                        bits_per_sample: 32,
                        sample_format: hound::SampleFormat::Float,
                    },
                )?;
                for sample in pcm.samples {
                    wav.write_sample(sample)?;
                }
                wav.finalize()?;
                Ok(())
            })();
            let _ = reply.send(generated);
        })?;
    let _owner = DesignThread {
        cancelled,
        handle: Some(handle),
    };
    result
        .await
        .map_err(|_| anyhow::anyhow!("MOSS design thread exited"))?
}
struct DesignThread {
    cancelled: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}
impl Drop for DesignThread {
    fn drop(&mut self) {
        self.cancelled.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}
