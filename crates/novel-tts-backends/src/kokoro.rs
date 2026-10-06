pub mod models;
mod voices;
use tts_core::backend::{AudioChunk, Backend, BackendError, Pcm, Streaming};
use tts_protocol::Capabilities;
/// Existing Kokoro implementation; its inference types stay private.
pub struct KokoroBackend {
    requests: tokio::sync::mpsc::Sender<Work>,
}

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
            default_voice: "Zf001".into(),
            voice_names: Default::default(),
            backend: "kokoro".into(),
            voices: voices::names(),
            native_streaming: false,
            style: false,
            cloning: false,
            pronunciation: false,
        }
    }
}

impl Backend for KokoroBackend {
    fn capabilities(&self) -> Capabilities {
        Self::capabilities()
    }

    fn stream<'a>(&'a self, text: &'a str, voice: &'a str) -> Streaming<'a> {
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
            let audio = result
                .await
                .map_err(|_| BackendError::Synthesis("inference worker exited".into()))??;
            let (tx, rx) = tokio::sync::mpsc::channel(2);
            tx.send(Ok(AudioChunk::Pcm(audio)))
                .await
                .map_err(|_| BackendError::Synthesis("stream closed".into()))?;
            tx.send(Ok(AudioChunk::End))
                .await
                .map_err(|_| BackendError::Synthesis("stream closed".into()))?;
            Ok(rx)
        })
    }
}

struct Work {
    text: String,
    voice: String,
    reply: tokio::sync::oneshot::Sender<Result<Pcm, BackendError>>,
}
