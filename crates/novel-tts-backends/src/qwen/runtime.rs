//! Model ownership, generation and recovery are confined to this thread.
use super::{MAX_FRAMES, available_devices, compiled_devices};
use qwen3_tts::{Language, Qwen3TTS, Speaker, SynthesisOptions};
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};
use tts_core::backend::{AudioChunk, BackendError, Pcm};
use tts_protocol::{Device, Event};

pub(super) struct Request {
    pub(super) text: String,
    pub(super) speaker: Speaker,
    pub(super) audio: mpsc::Sender<Result<AudioChunk, BackendError>>,
}

pub(super) fn run(
    directory: PathBuf,
    device: Device,
    recovery: Option<mpsc::Sender<Event>>,
    mut jobs: mpsc::Receiver<Request>,
    ready: oneshot::Sender<Result<(), BackendError>>,
) {
    let model = load(&directory, device);
    let mut model = match model {
        Ok(model) => model,
        Err(error) => {
            let _ = ready.send(Err(BackendError::Initialize(error.to_string())));
            return;
        }
    };
    if ready.send(Ok(())).is_err() {
        return;
    }
    let mut active_device = device;
    while let Some(request) = jobs.blocking_recv() {
        if request.audio.is_closed() {
            continue;
        }
        if let Err(error) = generate(&model, &request) {
            let recover = !request.audio.is_closed()
                && recovery.is_some()
                && active_device != Device::Cpu
                && matches!(error, BackendError::Synthesis(_));
            let _ = request.audio.blocking_send(Err(error));
            if recover {
                match load(&directory, Device::Cpu) {
                    Ok(cpu) => {
                        model = cpu;
                        active_device = Device::Cpu;
                        if let Some(progress) = &recovery {
                            let status = Event::DeviceStatus {
                                component: "tts".into(),
                                compiled: compiled_devices(),
                                available: available_devices(),
                                selected: Device::Cpu,
                                reason: Some("Qwen accelerator inference failed; CPU rebuilt for the next explicit playback; incomplete audio was not replayed".into()),
                            };
                            let _ = progress.blocking_send(status);
                        }
                    }
                    Err(_) => return,
                }
            }
        }
    }
}

fn load(directory: &std::path::Path, selected: Device) -> anyhow::Result<Qwen3TTS> {
    let device = match selected {
        Device::Cpu => qwen3_tts::parse_device("cpu")?,
        #[cfg(all(feature = "metal", target_os = "macos"))]
        Device::Metal => candle_metal::Device::new_metal(0)?,
        _ => anyhow::bail!("Qwen device {selected:?} is unavailable"),
    };
    Qwen3TTS::from_pretrained(&directory.to_string_lossy(), device)
}

fn inference_error(error: anyhow::Error) -> BackendError {
    BackendError::Synthesis(error.to_string())
}
fn generate(model: &Qwen3TTS, request: &Request) -> Result<(), BackendError> {
    let language = if request
        .text
        .chars()
        .any(|c| matches!(c, '\u{3400}'..='\u{9fff}'))
    {
        Language::Chinese
    } else {
        Language::English
    };
    let initialization = std::time::Instant::now();
    let mut stream = model
        .synthesize_streaming(
            &request.text,
            request.speaker,
            language,
            SynthesisOptions {
                max_length: MAX_FRAMES,
                chunk_frames: 10,
                seed: Some(42),
                ..Default::default()
            },
        )
        .map_err(inference_error)?;
    let mut emitted = false;
    let diagnostics = std::env::var("NOVEL_TTS_DIAGNOSTICS").is_ok_and(|value| value == "1");
    if diagnostics {
        eprintln!(
            "qwen initialization_ms={}",
            initialization.elapsed().as_millis()
        );
    }
    let mut chunk_index = 0;
    while !request.audio.is_closed() {
        let generated = std::time::Instant::now();
        let Some(audio) = stream.next_chunk().map_err(inference_error)? else {
            validate_completion(stream.is_done(), emitted)?;
            let _ = request.audio.blocking_send(Ok(AudioChunk::End));
            return Ok(());
        };
        let pcm = Pcm {
            samples: audio.samples,
            sample_rate: audio.sample_rate,
            channels: 1,
        };
        let duration_ms = pcm.duration_ms()?;
        let generation_ms = generated.elapsed().as_millis();
        chunk_index += 1;
        emitted = true;
        let waiting = std::time::Instant::now();
        let sent = request.audio.blocking_send(Ok(AudioChunk::Pcm(pcm)));
        if diagnostics {
            eprintln!(
                "qwen chunk={chunk_index} generation_ms={generation_ms} audio_ms={duration_ms} channel_wait_ms={} rtf={:.3}",
                waiting.elapsed().as_millis(),
                generation_ms as f64 / f64::from(duration_ms)
            );
        }
        if sent.is_err() {
            break;
        }
    }
    Ok(())
}
pub(super) fn validate_completion(eos: bool, emitted: bool) -> Result<(), BackendError> {
    if !eos {
        return Err(BackendError::Unsupported(
            "Qwen frame limit reached before end-of-speech; current segment is incomplete".into(),
        ));
    }
    if !emitted {
        return Err(BackendError::Unsupported("Qwen produced no audio".into()));
    }
    Ok(())
}
