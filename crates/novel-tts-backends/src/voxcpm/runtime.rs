use super::voice_store;
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};
use tts_core::backend::{AudioChunk, BackendError, Pcm};
use tts_protocol::Device;
pub(super) struct Request {
    pub text: String,
    pub voice: String,
    pub audio: mpsc::Sender<Result<AudioChunk, BackendError>>,
}
pub(super) fn native_device(device: Device) -> Option<voxcpm_sys::Device> {
    match device {
        Device::Cpu => Some(voxcpm_sys::Device::Cpu),
        Device::Cuda => Some(voxcpm_sys::Device::Cuda),
        Device::Metal => Some(voxcpm_sys::Device::Metal),
        _ => None,
    }
}
pub(super) fn run(
    directory: PathBuf,
    device: Device,
    mut jobs: mpsc::Receiver<Request>,
    ready: oneshot::Sender<Result<(), BackendError>>,
) {
    let mut model = match voxcpm_sys::Model::load(
        &directory.join("VoxCPM2-BaseLM-Q8_0.gguf"),
        &directory.join("VoxCPM2-Acoustic-F16.gguf"),
        native_device(device).expect("validated device"),
    ) {
        Ok(model) => model,
        Err(error) => {
            let _ = ready.send(Err(BackendError::Initialize(error)));
            return;
        }
    };
    if ready.send(Ok(())).is_err() {
        return;
    }
    while let Some(request) = jobs.blocking_recv() {
        if request.audio.is_closed() {
            continue;
        }
        if let Err(error) = generate(&mut model, &directory, &request) {
            let _ = request.audio.blocking_send(Err(error));
        }
    }
}
fn generate(
    model: &mut voxcpm_sys::Model,
    directory: &std::path::Path,
    request: &Request,
) -> Result<(), BackendError> {
    let reference = if request.voice.starts_with("custom:") {
        let store = voice_store(directory).map_err(|e| BackendError::Unsupported(e.to_string()))?;
        let voice = store
            .load(&request.voice)
            .map_err(|e| BackendError::Unsupported(e.to_string()))?;
        let path = store
            .path(&request.voice)
            .map_err(|e| BackendError::Unsupported(e.to_string()))?;
        let cache = path.join("features.json");
        let features: Vec<f32> = if cache.exists() {
            serde_json::from_slice(
                &std::fs::read(&cache).map_err(|e| BackendError::Synthesis(e.to_string()))?,
            )
            .map_err(|e| BackendError::Synthesis(e.to_string()))?
        } else {
            let audio = crate::reference::load(&path.join("reference.wav"), 16000)
                .map_err(|e| BackendError::Unsupported(e.to_string()))?;
            let features = model
                .encode_reference(&audio, 16000)
                .map_err(BackendError::Synthesis)?;
            if request.audio.is_closed() {
                return Ok(());
            }
            let temporary = tempfile::NamedTempFile::new_in(&path)
                .map_err(|e| BackendError::Synthesis(e.to_string()))?;
            serde_json::to_writer(temporary.as_file(), &features)
                .map_err(|e| BackendError::Synthesis(e.to_string()))?;
            temporary
                .persist(&cache)
                .map_err(|e| BackendError::Synthesis(e.to_string()))?;
            features
        };
        Some((features, voice.transcript))
    } else {
        None
    };
    let started = std::time::Instant::now();
    let mut channel_wait = std::time::Duration::ZERO;
    let mut audio_ms = 0u64;
    let mut emitted = false;
    let mut invalid = None;
    let result = model
        .generate(
            &request.text,
            reference
                .as_ref()
                .map(|(audio, transcript)| voxcpm_sys::Reference {
                    samples: audio,
                    sample_rate: 0,
                    transcript,
                    encoded: true,
                }),
            200,
            |samples| {
                if request.audio.is_closed() {
                    return false;
                }
                if samples.is_empty() {
                    return true;
                }
                let pcm = Pcm {
                    samples: samples.to_vec(),
                    sample_rate: 48000,
                    channels: 1,
                };
                if let Err(error) = pcm.duration_ms() {
                    invalid = Some(error);
                    return false;
                }
                emitted = true;
                audio_ms += u64::from(pcm.duration_ms().expect("validated PCM"));
                let waiting = std::time::Instant::now();
                let sent = request
                    .audio
                    .blocking_send(Ok(AudioChunk::Pcm(pcm)))
                    .is_ok();
                channel_wait += waiting.elapsed();
                sent
            },
        )
        .map_err(BackendError::Synthesis)?;
    if let Some(error) = invalid {
        return Err(error);
    }
    if request.audio.is_closed() {
        return Ok(());
    }
    match result {
        voxcpm_sys::Outcome::Complete if emitted => {
            if std::env::var("NOVEL_TTS_DIAGNOSTICS").is_ok_and(|v| v == "1") {
                eprintln!(
                    "voxcpm generation_ms={} audio_ms={audio_ms} channel_wait_ms={}",
                    started.elapsed().saturating_sub(channel_wait).as_millis(),
                    channel_wait.as_millis()
                );
            }
            let _ = request.audio.blocking_send(Ok(AudioChunk::End));
            Ok(())
        }
        voxcpm_sys::Outcome::Truncated => Err(BackendError::Unsupported(
            "Vox frame limit reached before EOS; segment is incomplete".into(),
        )),
        _ => Err(BackendError::Synthesis(
            "Vox produced no complete audio".into(),
        )),
    }
}
