//! Candle adapter; model tensors remain on a dedicated inference thread.
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
        pronunciation: false,
    }
}

pub fn compiled_devices() -> Vec<Device> {
    vec![
        Device::Cpu,
        #[cfg(all(feature = "metal", target_os = "macos"))]
        Device::Metal,
    ]
}
pub fn available_devices() -> Vec<Device> {
    compiled_devices()
        .into_iter()
        .filter(|device| match device {
            Device::Cpu => true,
            #[cfg(all(feature = "metal", target_os = "macos"))]
            Device::Metal => candle_metal::Device::new_metal(0).is_ok(),
            _ => false,
        })
        .collect()
}

pub struct QwenBackend {
    requests: mpsc::Sender<Request>,
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
        if !available_devices().contains(&device) {
            return Err(BackendError::Initialize(format!(
                "Qwen device {device:?} is unavailable"
            )));
        }
        let (requests, jobs) = mpsc::channel::<Request>(1);
        let (ready, loaded) = oneshot::channel();
        std::thread::Builder::new()
            .name("qwen-inference".into())
            .spawn(move || {
                runtime::run(directory, device, recovery, jobs, ready);
            })
            .map_err(|error| BackendError::Initialize(error.to_string()))?;
        loaded
            .await
            .map_err(|_| BackendError::Initialize("Qwen inference thread exited".into()))??;
        Ok(Self { requests })
    }
}

impl Backend for QwenBackend {
    fn capabilities(&self) -> Capabilities {
        capabilities()
    }
    fn stream<'a>(&'a self, text: &'a str, voice: &'a str) -> Streaming<'a> {
        Box::pin(async move {
            if !VOICES.contains(&voice) {
                return Err(BackendError::Unsupported(format!(
                    "unknown Qwen voice {voice}"
                )));
            }
            let speaker = voice
                .parse()
                .map_err(|error: anyhow::Error| BackendError::Unsupported(error.to_string()))?;
            let text = text::normalize(text);
            if text.is_empty() {
                return Err(BackendError::Unsupported(
                    "empty Qwen synthesis text".into(),
                ));
            }
            let (audio, receiver) = mpsc::channel(1);
            self.requests
                .send(Request {
                    text,
                    speaker,
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
        assert!(!compiled_devices().contains(&Device::Cuda));
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
