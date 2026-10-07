//! Candle adapter; model tensors remain on a dedicated inference thread.
pub mod design;
pub mod models;
pub mod resources;
mod runtime;
mod text;
use runtime::Request;

use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};
use tts_core::backend::{Backend, BackendError, Segmentation, Streaming};
use tts_protocol::{Capabilities, Device};

const VOICES: [&str; 9] = [
    "uncle_fu", "serena", "vivian", "ryan", "aiden", "ono_anna", "sohee", "eric", "dylan",
];
const MAX_FRAMES: usize = 375;

pub fn capabilities() -> Capabilities {
    Capabilities {
        model: Some(models::Model::Custom06.id().into()),
        model_name: models::Model::Custom06.name().into(),
        backend: "qwen".into(),
        voices: VOICES.iter().map(|voice| (*voice).into()).collect(),
        default_voice: "uncle_fu".into(),
        voice_names: VOICES
            .iter()
            .zip([
                "福叔（中文男声）",
                "Serena（中文女声）",
                "Vivian（中文女声）",
                "Ryan（英文男声）",
                "Aiden（英文男声）",
                "Anna（日语女声）",
                "Sohee（韩语女声）",
                "Eric（四川话男声）",
                "Dylan（北京话男声）",
            ])
            .map(|(id, name)| ((*id).into(), name.into()))
            .collect(),
        native_streaming: true,
        cloning: false,
        style: false,
        compiled_devices: Vec::new(),
        pronunciation: false,
    }
}
pub fn model_capabilities(model: models::Model) -> Capabilities {
    let mut caps = capabilities();
    caps.model = Some(model.id().into());
    caps.model_name = model.name().into();
    caps.style = model == models::Model::Custom17;
    if model == models::Model::Base17 {
        caps.voices.clear();
        caps.voice_names.clear();
        caps.default_voice.clear();
        caps.cloning = true;
    }
    caps
}
pub fn voice_store(
    directory: &std::path::Path,
    model: models::Model,
) -> anyhow::Result<tts_core::voices::VoiceStore> {
    tts_core::voices::VoiceStore::new(directory, "qwen", model.id(), model.revision())
}
pub fn validate_reference(audio: &qwen3_tts::AudioBuffer) -> anyhow::Result<()> {
    anyhow::ensure!(
        audio.sample_rate >= 8000
            && audio.sample_rate <= 192000
            && !audio.samples.is_empty()
            && audio.samples.iter().all(|value| value.is_finite()),
        "invalid WAV reference: use finite mono audio at 8..192 kHz"
    );
    let seconds = audio.samples.len() as f64 / audio.sample_rate as f64;
    anyhow::ensure!(
        (1.0..=15.0).contains(&seconds),
        "Qwen reference duration must be 1..15 seconds"
    );
    anyhow::ensure!(
        audio.samples.iter().any(|value| value.abs() > 0.001),
        "reference WAV contains only silence"
    );
    Ok(())
}
pub fn validate_wav(path: &std::path::Path) -> anyhow::Result<()> {
    validate_reference(&qwen3_tts::AudioBuffer::load(path)?)
}
pub fn capabilities_at(
    directory: &std::path::Path,
    model: models::Model,
) -> anyhow::Result<Capabilities> {
    let mut caps = model_capabilities(model);
    if model == models::Model::Base17 {
        for voice in voice_store(directory, model)?.list()? {
            caps.voice_names.insert(voice.id.clone(), voice.name);
            caps.voices.push(voice.id);
        }
        caps.default_voice = caps.voices.first().cloned().unwrap_or_default();
    }
    Ok(caps)
}
pub fn catalog(root: &std::path::Path) -> anyhow::Result<Vec<Capabilities>> {
    [
        models::Model::Custom06,
        models::Model::Custom17,
        models::Model::Base17,
    ]
    .into_iter()
    .map(|model| capabilities_at(&model.directory(root), model))
    .collect()
}

pub fn compiled_devices() -> Vec<Device> {
    vec![
        Device::Cpu,
        #[cfg(all(feature = "qwen-cuda", any(target_os = "windows", target_os = "linux")))]
        Device::Cuda,
        #[cfg(all(feature = "metal", target_os = "macos"))]
        Device::Metal,
    ]
}
pub fn available_devices() -> Vec<Device> {
    compiled_devices()
        .into_iter()
        .filter(|device| match device {
            Device::Cpu => true,
            #[cfg(all(feature = "qwen-cuda", any(target_os = "windows", target_os = "linux")))]
            Device::Cuda => qwen3_tts::device::cuda(0).is_ok(),
            #[cfg(all(feature = "metal", target_os = "macos"))]
            Device::Metal => qwen3_tts::device::metal(0).is_ok(),
            _ => false,
        })
        .collect()
}

pub struct QwenBackend {
    requests: Option<mpsc::Sender<Request>>,
    thread: Option<std::thread::JoinHandle<()>>,
    model: models::Model,
    directory: PathBuf,
}
impl QwenBackend {
    pub async fn load_on(directory: PathBuf, device: Device) -> Result<Self, BackendError> {
        Self::load_with_recovery(directory, device, None).await
    }
    pub async fn load_with_recovery(
        directory: PathBuf,
        device: Device,
        recovery: Option<mpsc::Sender<tts_protocol::Event>>,
    ) -> Result<Self, BackendError> {
        let model = models::Model::detect(&directory)
            .map_err(|e| BackendError::Initialize(e.to_string()))?;
        if !matches!(
            model,
            models::Model::Custom06 | models::Model::Custom17 | models::Model::Base17
        ) {
            return Err(BackendError::Unsupported(
                "this Qwen variant requires voice creation support".into(),
            ));
        }
        if !available_devices().contains(&device) {
            return Err(BackendError::Initialize(format!(
                "Qwen device {device:?} is unavailable"
            )));
        }
        let (requests, jobs) = mpsc::channel::<Request>(1);
        let (ready, loaded) = oneshot::channel();
        let model_directory = directory.clone();
        let thread = std::thread::Builder::new()
            .name("qwen-inference".into())
            .spawn(move || {
                runtime::run(directory, device, recovery, jobs, ready);
            })
            .map_err(|error| BackendError::Initialize(error.to_string()))?;
        // Own the thread before awaiting initialization: cancellation must join it.
        let backend = Self {
            requests: Some(requests),
            thread: Some(thread),
            model,
            directory: model_directory,
        };
        loaded
            .await
            .map_err(|_| BackendError::Initialize("Qwen inference thread exited".into()))??;
        Ok(backend)
    }
}

impl Backend for QwenBackend {
    fn capabilities(&self) -> Capabilities {
        // A corrupt voice record is reported by catalog/stream validation; do not invent a voice.
        capabilities_at(&self.directory, self.model)
            .unwrap_or_else(|_| model_capabilities(self.model))
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
            if style.is_some_and(|v| !v.trim().is_empty())
                && (self.model != models::Model::Custom17
                    || style.is_some_and(|v| v.chars().count() > 200))
            {
                return Err(BackendError::Unsupported(
                    "style requires Qwen 1.7B CustomVoice, up to 200 characters".into(),
                ));
            }
            let valid = if self.model == models::Model::Base17 {
                voice_store(&self.directory, self.model)
                    .and_then(|store| store.load(voice))
                    .is_ok()
            } else {
                VOICES.contains(&voice)
            };
            if !valid {
                return Err(BackendError::Unsupported(format!(
                    "unknown Qwen voice {voice}"
                )));
            }
            let text = text::normalize(text);
            if text.is_empty() {
                return Err(BackendError::Unsupported(
                    "empty Qwen synthesis text".into(),
                ));
            }
            let (audio, receiver) = mpsc::channel(1);
            self.requests
                .as_ref()
                .expect("inference thread exists while the backend is alive")
                .send(Request {
                    text,
                    voice: voice.into(),
                    style: style.filter(|v| !v.trim().is_empty()).map(str::to_owned),
                    audio,
                })
                .await
                .map_err(|_| BackendError::Synthesis("Qwen inference thread exited".into()))?;
            Ok(receiver)
        })
    }
    fn segments<'a>(&'a self, source: &'a str) -> Segmentation<'a> {
        Box::pin(async move { Ok(text::segments(source)) })
    }
    fn paragraph_end(&self, segment: &str, remaining: &str) -> bool {
        remaining.trim().is_empty()
            || tts_core::text::is_heading_line(segment.trim())
            || remaining.starts_with('\n') && segment.ends_with('\n')
    }
}
impl Drop for QwenBackend {
    fn drop(&mut self) {
        // Session teardown drops the audio receiver first, releasing bounded sends.
        self.requests.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qwen3_tts::Speaker;
    use runtime::validate_completion;
    use tts_core::backend::AudioChunk;
    #[test]
    fn voices_and_devices_match_adapter() {
        let caps = capabilities();
        assert!(caps.voices.contains(&caps.default_voice));
        for voice in caps.voices {
            assert!(voice.parse::<Speaker>().is_ok());
        }
        assert!(available_devices().contains(&Device::Cpu));
        assert!(!compiled_devices().contains(&Device::Coreml));
        assert_eq!(
            compiled_devices().contains(&Device::Cuda),
            cfg!(all(
                feature = "qwen-cuda",
                any(target_os = "windows", target_os = "linux")
            ))
        );
    }
    #[test]
    fn only_eos_with_pcm_completes_segment() {
        assert!(validate_completion(true, true).is_ok());
        assert!(validate_completion(false, true).is_err());
        assert!(validate_completion(true, false).is_err());
    }
    #[tokio::test]
    async fn real_model_streams_and_releases_cancelled_request() {
        let Some(directory) = std::env::var_os("TRNOVEL_QWEN_MODEL_DIR") else {
            return;
        };
        let backend = QwenBackend::load_on(directory.into(), Device::Cpu)
            .await
            .unwrap();
        assert!(backend.stream("hello", "invalid").await.is_err());
        let mut stream = backend.stream("你好。", "uncle_fu").await.unwrap();
        let pcm = match stream.recv().await.unwrap().unwrap() {
            AudioChunk::Pcm(pcm) => pcm,
            _ => panic!("missing PCM"),
        };
        assert_eq!(pcm.sample_rate, 24000);
        pcm.duration_ms().unwrap();
        drop(stream);
        let mut stream = backend.stream("谢谢。", "uncle_fu").await.unwrap();
        let mut samples = 0;
        while let Some(chunk) = stream.recv().await {
            match chunk.unwrap() {
                AudioChunk::Pcm(pcm) => samples += pcm.samples.len(),
                AudioChunk::End => {
                    assert!(samples > 0);
                    return;
                }
            }
        }
        panic!("no EOS");
    }
}
