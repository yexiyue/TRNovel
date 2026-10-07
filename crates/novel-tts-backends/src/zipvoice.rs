//! CPU Distill inference; semantic segments, never advertised as native streaming.
pub mod audio;
pub mod frontend;
pub mod model;
pub mod resources;
mod runtime;
use std::path::{Path, PathBuf};
use tokio::sync::{mpsc, oneshot};
use tts_core::backend::{Backend, BackendError, Segmentation, Streaming};
use tts_protocol::{Capabilities, Device};
#[derive(Clone, Copy)]
pub enum Variant {
    Int8,
    Fp32,
}
impl Variant {
    pub fn parse(value: Option<&str>) -> anyhow::Result<Self> {
        match value {
            None | Some("distill-int8") => Ok(Self::Int8),
            Some("distill-fp32") => Ok(Self::Fp32),
            _ => anyhow::bail!("unknown ZipVoice model {value:?}"),
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Int8 => "distill-int8",
            Self::Fp32 => "distill-fp32",
        }
    }
    pub fn directory(self, root: &Path) -> PathBuf {
        root.join("zipvoice/models")
            .join(self.id())
            .join(resources::REVISION)
    }
}
pub fn voice_store(
    directory: &Path,
    variant: Variant,
) -> anyhow::Result<tts_core::voices::VoiceStore> {
    tts_core::voices::VoiceStore::new(directory, "zipvoice", variant.id(), resources::REVISION)
}
pub fn capabilities(directory: &Path, variant: Variant) -> anyhow::Result<Capabilities> {
    let voices = voice_store(directory, variant)?.list()?;
    Ok(Capabilities {
        backend: "zipvoice".into(),
        model: Some(variant.id().into()),
        model_name: format!(
            "ZipVoice Distill {}",
            if matches!(variant, Variant::Int8) {
                "INT8"
            } else {
                "FP32"
            }
        ),
        voices: voices.iter().map(|v| v.id.clone()).collect(),
        default_voice: voices.first().map_or(String::new(), |v| v.id.clone()),
        voice_names: voices.into_iter().map(|v| (v.id, v.name)).collect(),
        native_streaming: false,
        cloning: true,
        style: false,
        compiled_devices: Vec::new(),
        pronunciation: false,
    })
}
pub struct ZipBackend {
    requests: Option<mpsc::Sender<runtime::Request>>,
    thread: Option<std::thread::JoinHandle<()>>,
    caps: Capabilities,
}
impl ZipBackend {
    pub async fn load_on(
        directory: PathBuf,
        variant: Variant,
        device: Device,
    ) -> Result<Self, BackendError> {
        if device != Device::Cpu {
            return Err(BackendError::Unsupported(
                "ZipVoice supports CPU only".into(),
            ));
        }
        let caps = capabilities(&directory, variant)
            .map_err(|e| BackendError::Initialize(e.to_string()))?;
        let (requests, jobs) = mpsc::channel(1);
        let (ready, loaded) = oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("zipvoice-inference".into())
            .spawn(move || runtime::run(directory, variant, jobs, ready))
            .map_err(|e| BackendError::Initialize(e.to_string()))?;
        // Own the thread before awaiting initialization: cancellation must join it.
        let backend = Self {
            requests: Some(requests),
            thread: Some(thread),
            caps,
        };
        loaded
            .await
            .map_err(|_| BackendError::Initialize("Zip inference thread exited".into()))??;
        Ok(backend)
    }
}
impl Drop for ZipBackend {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Backend for ZipBackend {
    fn capabilities(&self) -> Capabilities {
        self.caps.clone()
    }
    fn stream<'a>(&'a self, text: &'a str, voice: &'a str) -> Streaming<'a> {
        Box::pin(async move {
            if !self.caps.voices.iter().any(|id| id == voice) || text.trim().is_empty() {
                return Err(BackendError::Unsupported(
                    "ZipVoice requires an imported reference WAV and transcript".into(),
                ));
            }
            let (audio, receiver) = mpsc::channel(1);
            self.requests
                .as_ref()
                .expect("live inference thread")
                .send(runtime::Request {
                    text: text.into(),
                    voice: voice.into(),
                    audio,
                })
                .await
                .map_err(|_| BackendError::Synthesis("Zip inference thread exited".into()))?;
            Ok(receiver)
        })
    }
    fn segments<'a>(&'a self, source: &'a str) -> Segmentation<'a> {
        Box::pin(async move { Ok(tts_core::text::preprocess_text(source, 180)) })
    }
}
