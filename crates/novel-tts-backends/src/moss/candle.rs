//! GPU trial models share native computation and model-scoped reference voices.
pub mod design;
pub mod resources;
mod runtime;
use std::path::{Path, PathBuf};
use tokio::sync::{mpsc, oneshot};
use tts_core::backend::{Backend, BackendError, Segmentation, Streaming};
use tts_protocol::{Capabilities, Device};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Local,
    Realtime,
}
impl Mode {
    pub fn parse(id: &str) -> anyhow::Result<Self> {
        match id {
            "local-1.7b" => Ok(Self::Local),
            "realtime-1.7b" => Ok(Self::Realtime),
            _ => anyhow::bail!("unknown MOSS Candle model {id}"),
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Local => "local-1.7b",
            Self::Realtime => "realtime-1.7b",
        }
    }
    pub fn revision(self) -> &'static str {
        match self {
            Self::Local => "12aa734e4f11a7b3fdf4eb0ad2aa2029675ffc2e",
            Self::Realtime => "75682787d8e2fcc73faca37ba2931453ca9c4022",
        }
    }
    pub fn directory(self, root: &Path) -> PathBuf {
        root.join("moss/models")
            .join(self.id())
            .join(self.revision())
    }
}
pub fn voice_store(directory: &Path, mode: Mode) -> anyhow::Result<tts_core::voices::VoiceStore> {
    tts_core::voices::VoiceStore::new(directory, "moss", mode.id(), mode.revision())
}
pub fn capabilities(root: &Path, mode: Mode) -> anyhow::Result<Capabilities> {
    let mut caps = Capabilities {
        backend: "moss".into(),
        model: Some(mode.id().into()),
        model_name: match mode {
            Mode::Local => "MOSS Local 1.7B（实验，较慢）",
            Mode::Realtime => "MOSS Realtime 1.7B（实验）",
        }
        .into(),
        voices: vec!["narrator".into()],
        default_voice: "narrator".into(),
        voice_names: [("narrator".into(), "随机音色（建议导入或设计音色）".into())].into(),
        native_streaming: true,
        cloning: true,
        style: false,
        pronunciation: false,
        compiled_devices: compiled_devices(),
    };
    for voice in voice_store(&mode.directory(root), mode)?.list()? {
        caps.voice_names.insert(voice.id.clone(), voice.name);
        caps.voices.push(voice.id);
    }
    Ok(caps)
}
/// Production exposure is GPU-only until full CPU memory/performance acceptance.
pub fn compiled_devices() -> Vec<Device> {
    vec![
        #[cfg(all(
            feature = "moss-candle-cuda",
            any(target_os = "windows", target_os = "linux")
        ))]
        Device::Cuda,
        #[cfg(all(feature = "moss-candle-metal", target_os = "macos"))]
        Device::Metal,
    ]
}
pub fn available_devices() -> Vec<Device> {
    compiled_devices()
        .into_iter()
        .filter(|device| runtime::device(*device).is_ok())
        .collect()
}
pub struct CandleBackend {
    requests: Option<mpsc::Sender<runtime::Request>>,
    thread: Option<std::thread::JoinHandle<()>>,
    caps: Capabilities,
}
impl CandleBackend {
    pub async fn load_on(root: PathBuf, mode: Mode, device: Device) -> Result<Self, BackendError> {
        if !available_devices().contains(&device) {
            return Err(BackendError::Initialize(format!(
                "MOSS Candle device {device:?} is unavailable; select CUDA/Metal in a matching build"
            )));
        }
        let caps =
            capabilities(&root, mode).map_err(|e| BackendError::Initialize(e.to_string()))?;
        let (requests, jobs) = mpsc::channel(1);
        let (ready, loaded) = oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("moss-candle-inference".into())
            .spawn(move || runtime::run(root, mode, device, jobs, ready))
            .map_err(|e| BackendError::Initialize(e.to_string()))?;
        let backend = Self {
            requests: Some(requests),
            thread: Some(thread),
            caps,
        };
        loaded
            .await
            .map_err(|_| BackendError::Initialize("MOSS Candle thread exited".into()))??;
        Ok(backend)
    }
}
impl Drop for CandleBackend {
    fn drop(&mut self) {
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl Backend for CandleBackend {
    fn capabilities(&self) -> Capabilities {
        self.caps.clone()
    }
    fn stream<'a>(&'a self, text: &'a str, voice: &'a str) -> Streaming<'a> {
        Box::pin(async move {
            if text.trim().is_empty() || !self.caps.voices.iter().any(|id| id == voice) {
                return Err(BackendError::Unsupported(
                    "unknown MOSS voice or empty text".into(),
                ));
            }
            let (audio, receiver) = mpsc::channel(1);
            self.requests
                .as_ref()
                .expect("live inference owner")
                .send(runtime::Request {
                    text: text.into(),
                    voice: voice.into(),
                    audio,
                })
                .await
                .map_err(|_| BackendError::Synthesis("MOSS Candle thread exited".into()))?;
            Ok(receiver)
        })
    }
    fn segments<'a>(&'a self, source: &'a str) -> Segmentation<'a> {
        Box::pin(async move { Ok(tts_core::text::preprocess_text(source, 160)) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nano_identity_and_candle_devices_remain_separate() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let registry = crate::Registry::new(Some(root.path().into()))?;
        let nano = registry.capabilities_for("moss", None)?;
        assert_eq!(nano.model.as_deref(), Some("nano"));
        assert!(nano.matches("moss", None));
        assert_eq!(nano.default_voice, "Weiguo");
        assert_eq!(nano.compiled_devices, crate::devices::compiled());
        for mode in [Mode::Local, Mode::Realtime] {
            let caps = registry.capabilities_for("moss", Some(mode.id()))?;
            assert_eq!(caps.compiled_devices, compiled_devices());
            assert!(!caps.compiled_devices.contains(&Device::Cpu));
            assert_eq!(caps.default_voice, "narrator");
            assert!(caps.cloning && caps.native_streaming);
        }
        assert!(
            !root.path().join("moss").exists(),
            "catalog queries must not create assets"
        );
        Ok(())
    }
    #[test]
    fn modes_isolate_imported_voices_and_share_only_codec_assets() -> anyhow::Result<()> {
        let root = tempfile::tempdir()?;
        let wav = root.path().join("input.wav");
        std::fs::write(&wav, b"managed reference")?;
        let local = voice_store(&Mode::Local.directory(root.path()), Mode::Local)?;
        let realtime = voice_store(&Mode::Realtime.directory(root.path()), Mode::Realtime)?;
        local.import("custom:test", "test", &wav, "reference transcript", None)?;
        assert_eq!(local.list()?.len(), 1);
        assert!(realtime.list()?.is_empty());
        assert!(realtime.load("custom:test").is_err());
        assert!(
            super::super::voices::VoiceStore::new(&root.path().join("moss"))
                .list()?
                .is_empty()
        );
        assert_ne!(
            Mode::Local.directory(root.path()),
            Mode::Realtime.directory(root.path())
        );
        Ok(())
    }
}
