//! MOSS-TTS-Nano adapter. CPU inference and voice encoding share one owner thread.
mod audio;
#[cfg(feature = "moss-candle")]
pub mod candle;
pub mod diagnostics;
#[cfg(all(windows, feature = "directml-probe"))]
pub mod directml_probe;
pub mod resources;
mod runtime;
mod text;
pub mod voices;
use sentencepiece_rs::SentencePieceProcessor;
use std::{path::PathBuf, sync::Arc};
use tokio::sync::{mpsc, oneshot};
use tts_core::backend::{AudioChunk, Backend, BackendError, Segmentation, Streaming};
use tts_protocol::Capabilities;

pub fn capabilities(directory: &std::path::Path) -> anyhow::Result<Capabilities> {
    let manifest: serde_json::Value =
        serde_json::from_str(include_str!("moss/assets/browser_poc_manifest.json"))?;
    let mut voices = Vec::new();
    let mut names = std::collections::BTreeMap::new();
    for row in manifest["builtin_voices"].as_array().unwrap() {
        let id = row["voice"].as_str().unwrap().to_owned();
        names.insert(id.clone(), row["display_name"].as_str().unwrap().to_owned());
        voices.push(id);
    }
    for voice in voices::VoiceStore::new(directory).list()? {
        names.insert(voice.id.clone(), voice.name);
        voices.push(voice.id);
    }
    Ok(Capabilities {
        model: cfg!(feature = "moss-candle").then(|| "nano".into()),
        model_name: "MOSS Nano".into(),
        backend: "moss".into(),
        voices,
        default_voice: "Weiguo".into(),
        voice_names: names,
        native_streaming: true,
        cloning: true,
        style: false,
        compiled_devices: Vec::new(),
        pronunciation: false,
    })
}
enum Work {
    Generate {
        tokens: Vec<i32>,
        voice: String,
        seed: Option<u64>,
        audio: mpsc::Sender<Result<AudioChunk, BackendError>>,
        report: Option<(String, oneshot::Sender<diagnostics::GenerationReport>)>,
    },
    Import {
        id: String,
        name: String,
        wav: PathBuf,
        reply: oneshot::Sender<anyhow::Result<()>>,
    },
}
pub struct MossBackend {
    requests: mpsc::Sender<Work>,
    tokenizer: Arc<SentencePieceProcessor>,
    capabilities: Capabilities,
    estimate: std::cell::RefCell<(String, tts_core::text::duration::DurationEstimator)>,
}
impl MossBackend {
    pub async fn load(directory: PathBuf) -> Result<Self, BackendError> {
        Self::load_on(directory, tts_protocol::Device::Cpu).await
    }
    pub async fn load_on(
        directory: PathBuf,
        device: tts_protocol::Device,
    ) -> Result<Self, BackendError> {
        Self::load_with_recovery(directory, device, None).await
    }
    pub async fn load_with_recovery(
        directory: PathBuf,
        device: tts_protocol::Device,
        recovery: Option<mpsc::Sender<tts_protocol::Event>>,
    ) -> Result<Self, BackendError> {
        crate::devices::validate(device)
            .map_err(|error| BackendError::Initialize(error.to_string()))?;
        let caps = capabilities(&directory).map_err(|e| BackendError::Initialize(e.to_string()))?;
        let (requests, mut jobs) = mpsc::channel(1);
        let (ready, loaded) = oneshot::channel();
        std::thread::Builder::new()
            .name("moss-inference".into())
            .spawn(move || {
                let load = || -> anyhow::Result<_> {
                    Ok((
                        runtime::Runtime::load_on(&directory, || ready.is_closed(), device)?,
                        Arc::new(SentencePieceProcessor::open(
                            directory.join("tts/tokenizer.model"),
                        )?),
                    ))
                };
                let (mut runtime, tokenizer) = match load() {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = ready.send(Err(BackendError::Initialize(error.to_string())));
                        return;
                    }
                };
                if ready.send(Ok(tokenizer)).is_err() {
                    return;
                }
                let mut current_device = device;
                while let Some(job) = jobs.blocking_recv() {
                    match job {
                        Work::Generate {
                            tokens,
                            voice,
                            seed,
                            audio,
                            report,
                        } => {
                            use diagnostics::{GenerationEnd, GenerationReport, GenerationStats};
                            let token_count = tokens.len();
                            let mut stats = GenerationStats::default();
                            let result = if audio.is_closed() { Ok(GenerationEnd::Cancelled) }
                                else { runtime.generate(tokens, &voice, seed, &audio, &mut stats) };
                            let (end, error) = match result {
                                Ok(GenerationEnd::FrameLimit) => (GenerationEnd::FrameLimit, Some("MOSS frame limit reached before end-of-speech; block was not completed".to_string())),
                                Ok(end) => (end, None),
                                Err(_) if audio.is_closed() => (GenerationEnd::Cancelled, None),
                                Err(error) => (GenerationEnd::InferenceFailure, Some(error.to_string())),
                            };
                            if let Some((normalized_text, reply)) = report {
                                let _ = reply.send(GenerationReport { normalized_text, token_count,
                                    generated_frames: stats.frames, audio_seconds: stats.audio_seconds, end, error: error.clone() });
                            }
                            if let Some(error) = error {
                                let _ = audio.blocking_send(Err(BackendError::Synthesis(error.clone())));
                                if end == GenerationEnd::InferenceFailure
                                    && let Some(progress) = &recovery && current_device != tts_protocol::Device::Cpu {
                                    match runtime::Runtime::load_on(&directory, || false, tts_protocol::Device::Cpu) {
                                        Ok(cpu) => {
                                            runtime = cpu;
                                            current_device = tts_protocol::Device::Cpu;
                                            let _ = progress.blocking_send(crate::devices::status("tts", tts_protocol::Device::Cpu, Some(format!("inference failed; failed block was not replayed: {error}"))));
                                        }
                                        Err(_) => break,
                                    }
                                }
                            }
                        }

                        Work::Import {
                            id,
                            name,
                            wav,
                            reply,
                        } => {
                            if !reply.is_closed() {
                                let _ = reply.send(runtime.import_voice(id, name, &wav));
                            }
                        }
                    }
                }
            })
            .map_err(|e| BackendError::Initialize(e.to_string()))?;
        let tokenizer = loaded
            .await
            .map_err(|_| BackendError::Initialize("inference thread exited".into()))??;
        Ok(Self {
            requests,
            tokenizer,
            capabilities: caps,
            estimate: std::cell::RefCell::new((String::new(), Default::default())),
        })
    }
    pub async fn import_voice(&self, id: String, name: String, wav: PathBuf) -> anyhow::Result<()> {
        let (reply, result) = oneshot::channel();
        self.requests
            .send(Work::Import {
                id,
                name,
                wav,
                reply,
            })
            .await?;
        result.await?
    }
    /// Seeded streams support deterministic comparisons with the official runtime.
    pub async fn stream_seeded(
        &self,
        text: &str,
        voice: &str,
        seed: Option<u64>,
    ) -> Result<tts_core::backend::AudioStream, BackendError> {
        self.request(text, voice, seed, None).await
    }
    /// Generate optional evidence without exposing model details to core.
    pub async fn stream_diagnosed(
        &self,
        text: &str,
        voice: &str,
        seed: Option<u64>,
    ) -> Result<
        (
            tts_core::backend::AudioStream,
            oneshot::Receiver<diagnostics::GenerationReport>,
        ),
        BackendError,
    > {
        let (reply, report) = oneshot::channel();
        let stream = self.request(text, voice, seed, Some(reply)).await?;
        Ok((stream, report))
    }
    async fn request(
        &self,
        text: &str,
        voice: &str,
        seed: Option<u64>,
        report: Option<oneshot::Sender<diagnostics::GenerationReport>>,
    ) -> Result<tts_core::backend::AudioStream, BackendError> {
        let text = text::normalize(text);
        if text.is_empty() {
            return Err(BackendError::Unsupported("no speakable text".into()));
        }
        let tokens = self
            .tokenizer
            .encode_to_ids(&text)
            .map_err(|e| BackendError::Synthesis(e.to_string()))?
            .into_iter()
            .map(|id| id as i32)
            .collect();
        let (audio, stream) = mpsc::channel(1);
        self.requests
            .send(Work::Generate {
                tokens,
                voice: voice.into(),
                seed,
                audio,
                report: report.map(|reply| (text, reply)),
            })
            .await
            .map_err(|_| BackendError::Synthesis("inference thread exited".into()))?;
        Ok(stream)
    }
}
impl Backend for MossBackend {
    fn capabilities(&self) -> Capabilities {
        self.capabilities.clone()
    }
    fn stream<'a>(&'a self, text: &'a str, voice: &'a str) -> Streaming<'a> {
        Box::pin(self.stream_seeded(text, voice, None))
    }
    fn paragraph_end(&self, segment: &str, remaining: &str) -> bool {
        text::paragraph_end(segment, remaining)
    }
    fn select_voice(&self, voice: &str) {
        let mut estimate = self.estimate.borrow_mut();
        if estimate.0 != voice {
            *estimate = (voice.into(), Default::default());
        }
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
        Box::pin(
            async move { text::first_segment(text, &self.tokenizer, &self.estimate.borrow().1) },
        )
    }
    fn observe_duration(&self, text: &str, voice: &str, seconds: f64) {
        let mut estimate = self.estimate.borrow_mut();
        if estimate.0 != voice {
            *estimate = (voice.into(), Default::default());
        }
        estimate.1.observe(text, seconds);
    }
    fn segments<'a>(&'a self, text: &'a str) -> Segmentation<'a> {
        let text = text.to_owned();
        let tokenizer = self.tokenizer.clone();
        let estimate = self.estimate.borrow().1.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                text::segments_with_estimate(&text, &tokenizer, &estimate)
            })
            .await
            .map_err(|e| BackendError::Synthesis(e.to_string()))?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn real_diagnostics_distinguish_eos_cancel_and_inference_failure() {
        let Some(directory) = std::env::var_os("TRNOVEL_MOSS_MODEL_DIR") else {
            return;
        };
        let backend = MossBackend::load(directory.into()).await.unwrap();
        let (cancelled, report) = backend
            .stream_diagnosed("你好，欢迎使用听书功能。", "Weiguo", Some(42))
            .await
            .unwrap();
        drop(cancelled);
        assert_eq!(
            report.await.unwrap().end,
            diagnostics::GenerationEnd::Cancelled
        );
        let (mut stream, report) = backend
            .stream_diagnosed("你好。", "missing-voice", Some(42))
            .await
            .unwrap();
        assert!(stream.recv().await.unwrap().is_err());
        assert_eq!(
            report.await.unwrap().end,
            diagnostics::GenerationEnd::InferenceFailure
        );
        let (mut stream, report) = backend
            .stream_diagnosed("你好，欢迎使用听书功能。", "Weiguo", Some(42))
            .await
            .unwrap();
        while let Some(chunk) = stream.recv().await {
            if matches!(chunk.unwrap(), AudioChunk::End) {
                break;
            }
        }
        let report = report.await.unwrap();
        assert_eq!(report.end, diagnostics::GenerationEnd::Eos);
        assert!(report.generated_frames > 0 && report.audio_seconds > 0.0);
    }

    #[tokio::test]
    async fn real_model_matches_official_and_cancellation_releases_inference() {
        let Some(directory) = std::env::var_os("TRNOVEL_MOSS_MODEL_DIR") else {
            return;
        };
        let backend = MossBackend::load(directory.into()).await.unwrap();
        for fixture in [
            include_str!("../tests/fixtures/moss-official.json"),
            include_str!("../tests/fixtures/moss-official-mixed.json"),
        ] {
            let golden: serde_json::Value = serde_json::from_str(fixture).unwrap();
            let text = golden["text"].as_str().unwrap();
            let voice = golden["voice"].as_str().unwrap();
            let tokens = backend
                .tokenizer
                .encode_to_ids(&text::normalize(text))
                .unwrap();
            assert_eq!(serde_json::to_value(tokens).unwrap(), golden["tokens"]);
            let mut cancelled = backend.stream_seeded(text, voice, Some(42)).await.unwrap();
            assert!(matches!(
                cancelled.recv().await.unwrap().unwrap(),
                AudioChunk::Pcm(_)
            ));
            drop(cancelled);
            let mut stream = backend.stream_seeded(text, voice, Some(42)).await.unwrap();
            let mut samples = Vec::new();
            let mut ended = false;
            while let Some(chunk) =
                tokio::time::timeout(std::time::Duration::from_secs(30), stream.recv())
                    .await
                    .unwrap()
            {
                match chunk.unwrap() {
                    AudioChunk::Pcm(pcm) => {
                        assert_eq!((pcm.sample_rate, pcm.channels), (48000, 2));
                        samples.extend(pcm.samples);
                    }
                    AudioChunk::End => {
                        ended = true;
                        break;
                    }
                }
            }
            assert!(ended);
            assert_eq!(
                samples.len(),
                golden["sample_count"].as_u64().unwrap() as usize
            );
            for probe in golden["probes"].as_array().unwrap() {
                let index = probe["index"].as_u64().unwrap() as usize;
                let expected = probe["value"].as_f64().unwrap() as f32;
                assert!(
                    (samples[index] - expected).abs() < 1e-4,
                    "sample {index} differs from official reference"
                );
            }
        }
    }
}
