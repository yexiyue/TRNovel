//! Synthesis boundary with model-independent PCM and capabilities.
#[cfg(feature = "kokoro")]
mod voices;

use std::{future::Future, pin::Pin};
use tts_protocol::Capabilities;

/// Backend failures do not expose inference-library types to consumers.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("model initialization failed: {0}")]
    Initialize(String),
    #[error("synthesis failed: {0}")]
    Synthesis(String),
    #[error("invalid audio or unsupported capability: {0}")]
    Unsupported(String),
}

/// Interleaved float PCM; channels and sample rate belong to the backend.
#[derive(Debug)]
pub struct Pcm {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
}

impl Pcm {
    /// Validate framing and return the rounded-up duration in milliseconds.
    pub fn duration_ms(&self) -> Result<u32, BackendError> {
        if self.sample_rate == 0
            || self.channels == 0
            || self.samples.is_empty()
            || !self.samples.len().is_multiple_of(self.channels as usize)
            || self.samples.iter().any(|sample| !sample.is_finite())
        {
            return Err(BackendError::Unsupported(
                "invalid PCM format or samples".into(),
            ));
        }
        let frames = self.samples.len() as u64 / self.channels as u64;
        u32::try_from((frames * 1000).div_ceil(self.sample_rate as u64))
            .map_err(|_| BackendError::Unsupported("audio duration overflow".into()))
    }
}

/// Local future: model objects and audio devices stay on their owning thread.
pub type Synthesis<'a> = Pin<Box<dyn Future<Output = Result<Pcm, BackendError>> + 'a>>;

/// Domain interface shared by real and deterministic testing backends.
pub trait Backend {
    fn capabilities(&self) -> Capabilities;
    fn synthesize<'a>(&'a self, text: &'a str, voice: &'a str) -> Synthesis<'a>;
}

/// Existing Kokoro implementation; its inference types stay private.
#[cfg(feature = "kokoro")]
pub struct KokoroBackend {
    requests: tokio::sync::mpsc::Sender<Work>,
}

#[cfg(feature = "kokoro")]
impl KokoroBackend {
    /// Load already prepared resources; this method never downloads models.
    pub async fn load(
        model: &std::path::Path,
        voices: &std::path::Path,
    ) -> Result<Self, BackendError> {
        let model_path = model.to_path_buf();
        let voices_path = voices.to_path_buf();
        let (requests, mut jobs) = tokio::sync::mpsc::channel::<Work>(1);
        let (ready, loaded) = tokio::sync::oneshot::channel();
        // Construct and destroy the native model on one dedicated thread. Only
        // owned text and PCM cross this channel, never model or device handles.
        std::thread::Builder::new()
            .name("kokoro-inference".into())
            .spawn(move || {
                let runtime = match tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        let _ = ready.send(Err(BackendError::Initialize(error.to_string())));
                        return;
                    }
                };
                runtime.block_on(async move {
                    let model = match kokoro_tts::KokoroTts::new(model_path, voices_path).await {
                        Ok(model) => model,
                        Err(error) => {
                            let _ = ready.send(Err(BackendError::Initialize(error.to_string())));
                            return;
                        }
                    };
                    if ready.send(Ok(())).is_err() {
                        return;
                    }
                    while let Some(job) = jobs.recv().await {
                        if job.reply.is_closed() {
                            continue;
                        }
                        let result = match voices::parse(&job.voice) {
                            Ok(voice) => model
                                .synth(&job.text, voice)
                                .await
                                .map(|(samples, _)| Pcm {
                                    samples,
                                    sample_rate: 24000,
                                    channels: 1,
                                })
                                .map_err(|error| BackendError::Synthesis(error.to_string())),
                            Err(error) => Err(BackendError::Unsupported(error.to_string())),
                        };
                        let _ = job.reply.send(result);
                    }
                });
            })
            .map_err(|error| BackendError::Initialize(error.to_string()))?;
        loaded
            .await
            .map_err(|_| BackendError::Initialize("inference worker exited".into()))??;
        Ok(Self { requests })
    }

    /// Available stable voice IDs, retaining old configuration spellings.
    pub fn capabilities() -> Capabilities {
        Capabilities {
            backend: "kokoro".into(),
            voices: voices::names(),
            native_streaming: false,
            style: false,
            cloning: false,
            pronunciation: false,
        }
    }
}

#[cfg(feature = "kokoro")]
impl Backend for KokoroBackend {
    fn capabilities(&self) -> Capabilities {
        Self::capabilities()
    }

    fn synthesize<'a>(&'a self, text: &'a str, voice: &'a str) -> Synthesis<'a> {
        Box::pin(async move {
            let (reply, result) = tokio::sync::oneshot::channel();
            self.requests
                .send(Work {
                    text: text.into(),
                    voice: voice.into(),
                    reply,
                })
                .await
                .map_err(|_| BackendError::Synthesis("inference worker exited".into()))?;
            result
                .await
                .map_err(|_| BackendError::Synthesis("inference worker exited".into()))?
        })
    }
}

#[cfg(feature = "kokoro")]
struct Work {
    text: String,
    voice: String,
    reply: tokio::sync::oneshot::Sender<Result<Pcm, BackendError>>,
}
