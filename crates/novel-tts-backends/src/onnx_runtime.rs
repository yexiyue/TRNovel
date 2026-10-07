//! Dedicated ORT owners with bounded, cancellation-aware delivery.
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};
use tts_core::backend::{AudioChunk, Backend, BackendError, Segmentation, Streaming};
use tts_protocol::{Capabilities, Device};

pub(crate) struct Request {
    pub text: String,
    pub voice: String,
    pub style: Option<String>,
    pub audio: mpsc::Sender<Result<AudioChunk, BackendError>>,
}
pub(crate) trait Generator: Send {
    fn generate(&mut self, request: &Request, cancelled: &dyn Fn() -> bool) -> anyhow::Result<()>;
}
pub(crate) async fn verify_export(
    directory: &std::path::Path,
    revision: &str,
    source_revision: &str,
    required: &[&str],
    progress: mpsc::Sender<tts_protocol::Event>,
) -> anyhow::Result<()> {
    let path = directory.join("manifest.json");
    anyhow::ensure!(
        path.is_file(),
        "ONNX export missing at {}; run the documented export tool first",
        directory.display()
    );
    let manifest: serde_json::Value = serde_json::from_reader(std::fs::File::open(&path)?)?;
    anyhow::ensure!(
        manifest["abi_version"] == 1
            && manifest["export_revision"] == revision
            && manifest["source_revision"] == source_revision
            && manifest["precision"] == "fp32",
        "unsupported ONNX export identity"
    );
    let resources: Vec<crate::resources::Resource> =
        serde_json::from_value(manifest["resources"].clone())?;
    anyhow::ensure!(!resources.is_empty(), "empty export manifest");
    for path in required {
        anyhow::ensure!(
            resources.iter().any(|r| r.path == *path),
            "missing export resource {path}"
        );
    }
    for resource in &resources {
        anyhow::ensure!(
            !std::path::Path::new(&resource.path).is_absolute()
                && std::path::Path::new(&resource.path)
                    .components()
                    .all(|c| matches!(c, std::path::Component::Normal(_))),
            "invalid export resource path"
        );
    }
    crate::resources::prepare(directory, "onnx-experiment", resources, progress).await
}
pub struct OnnxBackend {
    requests: Option<mpsc::Sender<Request>>,
    thread: Option<std::thread::JoinHandle<()>>,
    caps: Capabilities,
}
impl OnnxBackend {
    pub(crate) async fn load(
        caps: Capabilities,
        directory: PathBuf,
        device: Device,
        loader: impl FnOnce(PathBuf, Device) -> anyhow::Result<Box<dyn Generator>> + Send + 'static,
    ) -> Result<Self, BackendError> {
        crate::devices::validate(device).map_err(|e| BackendError::Initialize(e.to_string()))?;
        let (requests, mut jobs) = mpsc::channel::<Request>(1);
        let (ready, loaded) = oneshot::channel();
        let thread = std::thread::Builder::new()
            .name(caps.backend.clone())
            .spawn(move || {
                let mut generator = match loader(directory, device) {
                    Ok(value) => value,
                    Err(error) => {
                        let _ = ready.send(Err(BackendError::Initialize(error.to_string())));
                        return;
                    }
                };
                if ready.send(Ok(())).is_err() {
                    return;
                }
                while let Some(request) = jobs.blocking_recv() {
                    let cancelled = || request.audio.is_closed() || jobs.is_closed();
                    if cancelled() {
                        continue;
                    }
                    if let Err(error) = generator.generate(&request, &cancelled)
                        && !cancelled()
                    {
                        let _ = send(
                            &request,
                            Err(BackendError::Synthesis(error.to_string())),
                            &cancelled,
                        );
                    }
                }
            })
            .map_err(|e| BackendError::Initialize(e.to_string()))?;
        // Own the join handle before awaiting initialization.
        let backend = Self {
            requests: Some(requests),
            thread: Some(thread),
            caps,
        };
        loaded.await.map_err(|_| {
            BackendError::Initialize("ORT owner exited during initialization".into())
        })??;
        Ok(backend)
    }
}
impl Drop for OnnxBackend {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Backend for OnnxBackend {
    fn capabilities(&self) -> Capabilities {
        self.caps.clone()
    }
    fn stream<'a>(&'a self, text: &'a str, voice: &'a str) -> Streaming<'a> {
        self.stream_with_style(text, voice, None)
    }
    fn stream_with_style<'a>(
        &'a self,
        text: &'a str,
        voice: &'a str,
        style: Option<&'a str>,
    ) -> Streaming<'a> {
        Box::pin(async move {
            if text.trim().is_empty() || !self.caps.voices.iter().any(|v| v == voice) {
                return Err(BackendError::Synthesis(
                    "empty text or unknown ONNX voice".into(),
                ));
            }
            if style.is_some_and(|s| {
                !s.trim().is_empty() && (!self.caps.style || s.chars().count() > 200)
            }) {
                return Err(BackendError::Synthesis(
                    "style unsupported or exceeds 200 characters".into(),
                ));
            }
            let (audio, receiver) = mpsc::channel(1);
            #[cfg(feature = "qwen-onnx")]
            let text = if self.caps.backend == "qwen-onnx" {
                crate::qwen::text::normalize(text)
            } else {
                text.to_owned()
            };
            #[cfg(not(feature = "qwen-onnx"))]
            let text = text.to_owned();
            self.requests
                .as_ref()
                .ok_or_else(|| BackendError::Synthesis("ORT owner closed".into()))?
                .send(Request {
                    text,
                    voice: voice.into(),
                    style: style.filter(|s| !s.trim().is_empty()).map(str::to_owned),
                    audio,
                })
                .await
                .map_err(|_| BackendError::Synthesis("ORT owner exited".into()))?;
            Ok(receiver)
        })
    }
    fn segments<'a>(&'a self, source: &'a str) -> Segmentation<'a> {
        Box::pin(async move {
            #[cfg(feature = "qwen-onnx")]
            if self.caps.backend == "qwen-onnx" {
                return Ok(crate::qwen::text::segments(source));
            }
            Ok(tts_core::text::preprocess_text(source, 180))
        })
    }
    fn paragraph_end(&self, segment: &str, remaining: &str) -> bool {
        #[cfg(feature = "qwen-onnx")]
        if self.caps.backend == "qwen-onnx" {
            return crate::qwen::text::paragraph_end(segment, remaining);
        }
        segment.ends_with('\n') || remaining.trim().is_empty()
    }
}
pub(crate) fn send(
    request: &Request,
    mut value: Result<AudioChunk, BackendError>,
    cancelled: &dyn Fn() -> bool,
) -> anyhow::Result<()> {
    loop {
        anyhow::ensure!(!cancelled(), "inference cancelled");
        match request.audio.try_send(value) {
            Ok(()) => return Ok(()),
            Err(mpsc::error::TrySendError::Closed(_)) => anyhow::bail!("audio receiver closed"),
            Err(mpsc::error::TrySendError::Full(item)) => {
                value = item;
                std::thread::sleep(std::time::Duration::from_millis(2));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[test]
    fn full_audio_queue_exits_when_owner_is_cancelled() {
        let (audio, _receiver) = mpsc::channel(1);
        audio.try_send(Ok(AudioChunk::End)).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        let flag = cancelled.clone();
        let thread = std::thread::spawn(move || {
            let request = Request {
                text: "test".into(),
                voice: "test".into(),
                style: None,
                audio,
            };
            send(&request, Ok(AudioChunk::End), &|| {
                flag.load(Ordering::Acquire)
            })
        });
        std::thread::sleep(std::time::Duration::from_millis(10));
        cancelled.store(true, Ordering::Release);
        assert!(thread.join().unwrap().is_err());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn cancelled_request_does_not_poison_owner() {
        struct Echo;
        impl Generator for Echo {
            fn generate(
                &mut self,
                request: &Request,
                cancelled: &dyn Fn() -> bool,
            ) -> anyhow::Result<()> {
                send(request, Ok(AudioChunk::End), cancelled)
            }
        }
        let caps = Capabilities {
            compiled_devices: vec![Device::Cpu],
            model: None,
            model_name: "test".into(),
            backend: "test".into(),
            voices: vec!["test".into()],
            default_voice: "test".into(),
            voice_names: Default::default(),
            native_streaming: false,
            style: false,
            cloning: false,
            pronunciation: false,
        };
        let backend =
            OnnxBackend::load(caps, PathBuf::new(), Device::Cpu, |_, _| Ok(Box::new(Echo)))
                .await
                .unwrap();
        drop(backend.stream("cancelled", "test").await.unwrap());
        let mut stream = backend.stream("next", "test").await.unwrap();
        assert!(matches!(
            stream.recv().await.unwrap().unwrap(),
            AudioChunk::End
        ));
    }
}
