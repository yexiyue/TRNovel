use super::cache::Cache;
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};
use tts_core::backend::{AudioChunk, BackendError, Pcm};
use tts_protocol::Device;
use voxcpm::{Model, Options, Outcome};
pub(super) struct Request {
    pub text: String,
    pub voice: String,
    pub audio: mpsc::Sender<Result<AudioChunk, BackendError>>,
}
pub(super) fn candle_device(device: Device) -> candle_core::Result<candle_core::Device> {
    if !super::compiled_devices().contains(&device) {
        candle_core::bail!("VoxCPM2 device {device:?} was not compiled");
    }
    match device {
        Device::Cpu => Ok(candle_core::Device::Cpu),
        Device::Cuda => candle_core::Device::new_cuda(0),
        Device::Metal => candle_core::Device::new_metal(0),
        _ => candle_core::bail!("unsupported VoxCPM2 device {device:?}"),
    }
}
pub(super) fn run(
    directory: PathBuf,
    variant: super::models::Model,
    device: Device,
    mut jobs: mpsc::Receiver<Request>,
    ready: oneshot::Sender<Result<(), BackendError>>,
) {
    let loaded = candle_device(device).and_then(|device| {
        variant
            .load(&directory, &device, &|| {
                ready.is_closed() || jobs.is_closed()
            })
            .map(|model| (device, model))
    });
    let (device, mut model) = match loaded {
        Ok(loaded) => loaded,
        Err(error) => {
            let _ = ready.send(Err(BackendError::Initialize(error.to_string())));
            return;
        }
    };
    if ready.send(Ok(())).is_err() {
        return;
    }
    let mut cache = Cache::default();
    while let Some(request) = jobs.blocking_recv() {
        if request.audio.is_closed() {
            continue;
        }
        if let Err(error) = generate(
            &mut model, variant, &device, &mut cache, &directory, &request,
        ) {
            let _ = request.audio.blocking_send(Err(error));
        }
    }
}
fn generate(
    model: &mut Model,
    variant: super::models::Model,
    device: &candle_core::Device,
    cache: &mut Cache,
    directory: &std::path::Path,
    request: &Request,
) -> Result<(), BackendError> {
    let cancel = || request.audio.is_closed();
    let reference = if request.voice.starts_with("custom:") {
        Some(
            cache
                .reference(model, variant, device, directory, &request.voice, &cancel)
                .map_err(|e| BackendError::Synthesis(e.to_string()))?,
        )
    } else {
        None
    };
    if cancel() {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let mut channel_wait = std::time::Duration::ZERO;
    let mut audio_ms = 0u64;
    let mut emitted = false;
    let mut invalid = None;
    let result = model
        .generate(
            &request.text,
            reference.as_ref(),
            &Options::default(),
            &cancel,
            |samples| {
                if request.audio.is_closed() {
                    return Ok(());
                }
                if samples.is_empty() {
                    return Ok(());
                }
                let pcm = Pcm {
                    samples: samples.to_vec(),
                    sample_rate: 48000,
                    channels: 1,
                };
                let duration = match pcm.duration_ms() {
                    Ok(duration) => duration,
                    Err(error) => {
                        invalid = Some(error);
                        candle_core::bail!("invalid Vox PCM");
                    }
                };
                let waiting = std::time::Instant::now();
                if request
                    .audio
                    .blocking_send(Ok(AudioChunk::Pcm(pcm)))
                    .is_ok()
                {
                    emitted = true;
                    audio_ms += u64::from(duration);
                }
                channel_wait += waiting.elapsed();
                Ok(())
            },
        )
        .map_err(|e| BackendError::Synthesis(e.to_string()))?;
    if let Some(error) = invalid {
        return Err(error);
    }
    if request.audio.is_closed() {
        return Ok(());
    }
    if completes_segment(result, emitted)? {
        if std::env::var("NOVEL_TTS_DIAGNOSTICS").is_ok_and(|v| v == "1") {
            eprintln!(
                "voxcpm-candle generation_ms={} audio_ms={audio_ms} channel_wait_ms={}",
                started.elapsed().saturating_sub(channel_wait).as_millis(),
                channel_wait.as_millis()
            );
        }
        let _ = request.audio.blocking_send(Ok(AudioChunk::End));
    }
    Ok(())
}
fn completes_segment(result: Outcome, emitted: bool) -> Result<bool, BackendError> {
    match result {
        Outcome::Eos if emitted => Ok(true),
        Outcome::Truncated => Err(BackendError::Unsupported(
            "Vox frame limit reached before EOS; segment is incomplete".into(),
        )),
        Outcome::Cancelled => Ok(false),
        _ => Err(BackendError::Synthesis(
            "Vox produced no complete audio".into(),
        )),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_truncated_or_empty_generation_never_completes_checkpoint() {
        assert!(completes_segment(Outcome::Eos, true).unwrap());
        assert!(completes_segment(Outcome::Eos, false).is_err());
        for emitted in [true, false] {
            assert!(!completes_segment(Outcome::Cancelled, emitted).unwrap());
            assert!(completes_segment(Outcome::Truncated, emitted).is_err());
        }
    }
    #[test]
    fn explicit_unsupported_devices_never_fall_back_to_cpu() {
        assert!(candle_device(Device::Auto).is_err());
        assert!(candle_device(Device::Coreml).is_err());
        assert!(candle_device(Device::Cpu).unwrap().is_cpu());
        if !super::super::compiled_devices().contains(&Device::Cuda) {
            assert!(candle_device(Device::Cuda).is_err());
        }
        if !super::super::compiled_devices().contains(&Device::Metal) {
            assert!(candle_device(Device::Metal).is_err());
        }
    }
}
