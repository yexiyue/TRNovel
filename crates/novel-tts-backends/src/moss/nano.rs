//! Experimental Candle Nano adapter; shares Nano voice codes with ONNX.
use candle_core::{DType, Device as CandleDevice};
use sentencepiece_rs::SentencePieceProcessor;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::{mpsc, oneshot};
use tts_core::backend::{AudioChunk, Backend, BackendError, Pcm, Segmentation, Streaming};
use tts_protocol::{Capabilities, Device, Event};
pub const REVISION: &str = "44502f80dbf9743528fa921cc544d662c685ebec";
pub fn directory(root: &Path) -> PathBuf {
    root.join("moss/models/nano-candle").join(REVISION)
}
pub fn capabilities(root: &Path) -> anyhow::Result<Capabilities> {
    let mut caps = super::capabilities(&root.join("moss"))?;
    caps.model = Some("nano-candle".into());
    caps.model_name = "MOSS Nano Candle（实验）".into();
    caps.compiled_devices = compiled_devices();
    Ok(caps)
}
pub fn compiled_devices() -> Vec<Device> {
    vec![
        Device::Cpu,
        #[cfg(all(feature = "moss-nano-candle-metal", target_os = "macos"))]
        Device::Metal,
        #[cfg(all(
            feature = "moss-nano-candle-cuda",
            any(target_os = "windows", target_os = "linux")
        ))]
        Device::Cuda,
    ]
}
fn device(device: Device) -> anyhow::Result<CandleDevice> {
    Ok(match device {
        Device::Cpu => CandleDevice::Cpu,
        #[cfg(all(feature = "moss-nano-candle-metal", target_os = "macos"))]
        Device::Metal => CandleDevice::new_metal(0)?,
        #[cfg(all(
            feature = "moss-nano-candle-cuda",
            any(target_os = "windows", target_os = "linux")
        ))]
        Device::Cuda => CandleDevice::new_cuda(0)?,
        _ => anyhow::bail!("Nano Candle device is not compiled"),
    })
}
pub fn available_devices() -> Vec<Device> {
    compiled_devices()
        .into_iter()
        .filter(|d| device(*d).is_ok())
        .collect()
}
pub async fn prepare(root: &Path, progress: mpsc::Sender<Event>) -> anyhow::Result<()> {
    crate::resources::prepare(
        &directory(root),
        "moss/nano-candle",
        serde_json::from_str(include_str!("nano/resources.json"))?,
        progress,
    )
    .await
}
struct Request {
    tokens: Vec<u32>,
    voice: String,
    seed: u64,
    audio: mpsc::Sender<Result<AudioChunk, BackendError>>,
}
pub struct NanoBackend {
    requests: Option<mpsc::Sender<Request>>,
    thread: Option<std::thread::JoinHandle<()>>,
    tokenizer: Option<Arc<SentencePieceProcessor>>,
    caps: Capabilities,
    estimate: std::cell::RefCell<(String, tts_core::text::duration::DurationEstimator)>,
}
impl NanoBackend {
    pub async fn load_on(root: PathBuf, selected: Device) -> Result<Self, BackendError> {
        let caps = capabilities(&root).map_err(|e| BackendError::Initialize(e.to_string()))?;
        let (requests, mut jobs) = mpsc::channel::<Request>(1);
        let (ready, loaded) = oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("moss-nano-candle".into())
            .spawn(move || {
                let load = || -> anyhow::Result<_> {
                    let device = device(selected)?;
                    let dir = directory(&root);
                    let model = moss_tts::nano::Nano::load(&dir.join("tts"), &device)?;
                    anyhow::ensure!(!ready.is_closed(), "Nano loading cancelled");
                    let codec =
                        moss_tts::codec::AudioCodec::load(&dir.join("codec"), &device, DType::F32)?;
                    let tokenizer = Arc::new(SentencePieceProcessor::open(
                        dir.join("tts/tokenizer.model"),
                    )?);
                    Ok((model, codec, tokenizer))
                };
                let (mut model, mut codec, tokenizer) = match load() {
                    Ok(v) => v,
                    Err(e) => {
                        let _ = ready.send(Err(BackendError::Initialize(e.to_string())));
                        return;
                    }
                };
                if ready.send(Ok(tokenizer)).is_err() {
                    return;
                }
                while let Some(job) = jobs.blocking_recv() {
                    if job.audio.is_closed() {
                        continue;
                    }
                    let result =
                        generate(&root, &mut model, &mut codec, &job, &|| jobs.is_closed());
                    if let Err(e) = result
                        && !job.audio.is_closed()
                        && !jobs.is_closed()
                    {
                        let _ = send_audio(
                            &job.audio,
                            Err(BackendError::Synthesis(e.to_string())),
                            &|| jobs.is_closed(),
                        );
                    }
                }
            })
            .map_err(|e| BackendError::Initialize(e.to_string()))?;
        // Acquire the owner before awaiting so cancellation always joins the thread.
        let mut backend = Self {
            requests: Some(requests),
            thread: Some(thread),
            tokenizer: None,
            caps,
            estimate: std::cell::RefCell::new((String::new(), Default::default())),
        };
        backend.tokenizer = Some(
            loaded
                .await
                .map_err(|_| BackendError::Initialize("Nano thread exited".into()))??,
        );
        Ok(backend)
    }
    pub async fn stream_seeded(
        &self,
        text: &str,
        voice: &str,
        seed: u64,
    ) -> Result<tts_core::backend::AudioStream, BackendError> {
        let text = super::text::normalize(text);
        if text.is_empty() || !self.caps.voices.iter().any(|v| v == voice) {
            return Err(BackendError::Unsupported(
                "empty text or unknown Nano voice".into(),
            ));
        }
        let tokens = self
            .tokenizer
            .as_ref()
            .expect("loaded tokenizer")
            .encode_to_ids(&text)
            .map_err(|e| BackendError::Synthesis(e.to_string()))?
            .into_iter()
            .map(|id| id as u32)
            .collect();
        let (audio, stream) = mpsc::channel(1);
        self.requests
            .as_ref()
            .expect("live Nano owner")
            .send(Request {
                tokens,
                voice: voice.into(),
                seed,
                audio,
            })
            .await
            .map_err(|_| BackendError::Synthesis("Nano thread exited".into()))?;
        Ok(stream)
    }
}
impl Drop for NanoBackend {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Backend for NanoBackend {
    fn capabilities(&self) -> Capabilities {
        self.caps.clone()
    }
    fn stream<'a>(&'a self, text: &'a str, voice: &'a str) -> Streaming<'a> {
        Box::pin(self.stream_seeded(text, voice, rand::random()))
    }
    fn paragraph_end(&self, segment: &str, remaining: &str) -> bool {
        super::text::paragraph_end(segment, remaining)
    }
    fn select_voice(&self, voice: &str) {
        let mut estimate = self.estimate.borrow_mut();
        if estimate.0 != voice {
            *estimate = (voice.into(), Default::default());
        }
    }
    fn observe_duration(&self, text: &str, voice: &str, seconds: f64) {
        self.select_voice(voice);
        self.estimate.borrow_mut().1.observe(text, seconds);
    }
    fn next_segment<'a>(
        &'a self,
        text: &'a str,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<
                    Output = Result<Option<tts_core::text::TextSegment>, BackendError>,
                > + 'a,
        >,
    > {
        Box::pin(async move {
            super::text::first_segment(
                text,
                self.tokenizer.as_ref().expect("loaded tokenizer"),
                &self.estimate.borrow().1,
            )
        })
    }
    fn segments<'a>(&'a self, text: &'a str) -> Segmentation<'a> {
        Box::pin(async move {
            super::text::segments_with_estimate(
                text,
                self.tokenizer.as_ref().expect("loaded tokenizer"),
                &self.estimate.borrow().1,
            )
        })
    }
}
fn generate(
    root: &Path,
    model: &mut moss_tts::nano::Nano,
    codec: &mut moss_tts::codec::AudioCodec,
    job: &Request,
    owner_closed: &impl Fn() -> bool,
) -> anyhow::Result<()> {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("assets/browser_poc_manifest.json"))?;
    let codes = super::voices::VoiceStore::new(&root.join("moss")).codes(&job.voice, &manifest)?;
    let mut rows = Vec::new();
    let text_row = |token| {
        let mut row = vec![token];
        row.extend([1024; 16]);
        row
    };
    let append = |rows: &mut Vec<Vec<u32>>, key: &str| -> anyhow::Result<()> {
        let ids: Vec<u32> = serde_json::from_value(manifest["prompt_templates"][key].clone())?;
        rows.extend(ids.into_iter().map(text_row));
        Ok(())
    };
    append(&mut rows, "user_prompt_prefix_token_ids")?;
    rows.push(text_row(6));
    for code in codes {
        let mut row = vec![8];
        row.extend(code.into_iter().map(|id| id as u32));
        rows.push(row);
    }
    rows.push(text_row(7));
    append(&mut rows, "user_prompt_after_reference_token_ids")?;
    rows.extend(job.tokens.iter().copied().map(text_row));
    append(&mut rows, "assistant_prompt_prefix_token_ids")?;
    rows.push(text_row(6));
    codec.reset_decoder();
    let mut pending = Vec::new();
    let mut generated = 0usize;
    let cancelled = || job.audio.is_closed() || owner_closed();
    let send = |codec: &mut moss_tts::codec::AudioCodec,
                pending: &mut Vec<Vec<u32>>|
     -> anyhow::Result<()> {
        let samples = codec.decode(pending, &cancelled)?;
        let pcm = Pcm {
            samples,
            sample_rate: codec.sample_rate,
            channels: codec.channels as u16,
        };
        pcm.duration_ms()?;
        send_audio(&job.audio, Ok(AudioChunk::Pcm(pcm)), &cancelled)?;
        pending.clear();
        Ok(())
    };
    model.generate(&rows, 375, job.seed, &cancelled, |frame| {
        generated += 1;
        pending.push(frame);
        if pending.len() == 3 {
            send(codec, &mut pending)?;
        }
        Ok(())
    })?;
    anyhow::ensure!(generated > 0, "Nano produced empty speech");
    if !pending.is_empty() {
        send(codec, &mut pending)?;
    }
    send_audio(&job.audio, Ok(AudioChunk::End), &cancelled)?;
    Ok(())
}

/// Keep bounded backpressure while allowing the owner to cancel a full queue.
fn send_audio(
    sender: &mpsc::Sender<Result<AudioChunk, BackendError>>,
    mut chunk: Result<AudioChunk, BackendError>,
    cancelled: &impl Fn() -> bool,
) -> anyhow::Result<()> {
    loop {
        anyhow::ensure!(!cancelled(), "Nano cancelled");
        match sender.try_send(chunk) {
            Ok(()) => return Ok(()),
            Err(mpsc::error::TrySendError::Closed(_)) => anyhow::bail!("Nano stream closed"),
            Err(mpsc::error::TrySendError::Full(value)) => {
                chunk = value;
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_releases_a_full_pcm_queue() {
        let (sender, _receiver) = mpsc::channel(1);
        sender.try_send(Ok(AudioChunk::End)).unwrap();
        let cancelled = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let signal = cancelled.clone();
        let thread = std::thread::spawn(move || {
            send_audio(&sender, Ok(AudioChunk::End), &|| {
                signal.load(std::sync::atomic::Ordering::Relaxed)
            })
        });
        cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
        assert!(thread.join().unwrap().is_err());
    }
    #[test]
    fn catalog_shares_nano_voices_and_keeps_onnx_as_default() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let registry = crate::Registry::new(Some(root.path().into()))?;
        let onnx = registry.capabilities_for("moss", None)?;
        let candle = registry.capabilities_for("moss", Some("nano-candle"))?;
        assert_eq!(onnx.model.as_deref(), Some("nano"));
        assert_eq!(onnx.voices, candle.voices);
        assert_eq!(candle.default_voice, "Weiguo");
        assert!(candle.compiled_devices.contains(&Device::Cpu));
        assert!(!root.path().join("moss").exists());
        super::super::voices::VoiceStore::new(&root.path().join("moss")).save(
            "custom:shared".into(),
            "shared".into(),
            vec![vec![42; 16]; 3],
        )?;
        assert!(
            registry
                .capabilities_for("moss", Some("nano-candle"))?
                .voices
                .iter()
                .any(|id| id == "custom:shared")
        );
        Ok(())
    }
}
