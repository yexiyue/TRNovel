//! Model ownership, generation and recovery are confined to this thread.
use super::{MAX_FRAMES, available_devices, compiled_devices};
use qwen3_tts::{Language, Qwen3TTS, Speaker, SynthesisOptions};
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};
use tts_core::backend::{AudioChunk, BackendError, Pcm};
use tts_protocol::{Device, Event};

pub(super) struct Request {
    pub(super) text: String,
    pub(super) voice: String,
    pub(super) style: Option<String>,
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
    let mut prompts = std::collections::HashMap::new();
    while let Some(request) = jobs.blocking_recv() {
        if request.audio.is_closed() {
            continue;
        }
        if let Err(error) = generate(&model, &directory, &mut prompts, &request) {
            let recover = !request.audio.is_closed()
                && recovery.is_some()
                && active_device != Device::Cpu
                && matches!(error, BackendError::Synthesis(_));
            let _ = request.audio.blocking_send(Err(error));
            if recover {
                match load(&directory, Device::Cpu) {
                    Ok(cpu) => {
                        model = cpu;
                        prompts.clear();
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

pub(super) fn load(directory: &std::path::Path, selected: Device) -> anyhow::Result<Qwen3TTS> {
    let device = match selected {
        Device::Cpu => qwen3_tts::Device::Cpu,
        #[cfg(all(feature = "qwen-cuda", any(target_os = "windows", target_os = "linux")))]
        Device::Cuda => qwen3_tts::device::cuda(0)?,
        #[cfg(all(feature = "metal", target_os = "macos"))]
        Device::Metal => qwen3_tts::device::metal(0)?,
        _ => anyhow::bail!("Qwen device {selected:?} is unavailable"),
    };
    Qwen3TTS::from_pretrained(&directory.to_string_lossy(), device)
}

fn inference_error(error: anyhow::Error) -> BackendError {
    BackendError::Synthesis(error.to_string())
}
fn generate(
    model: &Qwen3TTS,
    directory: &std::path::Path,
    prompts: &mut std::collections::HashMap<String, qwen3_tts::VoiceClonePrompt>,
    request: &Request,
) -> Result<(), BackendError> {
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
    let options = SynthesisOptions {
        max_length: MAX_FRAMES,
        chunk_frames: if model.device().is_cuda() { 20 } else { 10 },
        seed: Some(42),
        ..Default::default()
    };
    let mut stream = if request.voice.starts_with("custom:") {
        if !prompts.contains_key(&request.voice) {
            let variant = super::models::Model::detect(directory).map_err(inference_error)?;
            let store = super::voice_store(directory, variant).map_err(inference_error)?;
            let voice = store.load(&request.voice).map_err(inference_error)?;
            let path = store.path(&request.voice).map_err(inference_error)?;
            let cache = path.join("prompt.json");
            let prompt = if cache.exists() {
                qwen3_tts::VoiceClonePrompt::load(&cache, model.device())
                    .map_err(inference_error)?
            } else {
                let reference = qwen3_tts::AudioBuffer::load(path.join("reference.wav"))
                    .map_err(inference_error)?;
                super::validate_reference(&reference).map_err(inference_error)?;
                let prompt = model
                    .create_voice_clone_prompt(&reference, Some(&voice.transcript))
                    .map_err(inference_error)?;
                let temp = tempfile::NamedTempFile::new_in(&path)
                    .map_err(|e| inference_error(e.into()))?;
                prompt.save(temp.path()).map_err(inference_error)?;
                temp.persist(&cache)
                    .map_err(|e| inference_error(e.into()))?;
                prompt
            };
            prompts.insert(request.voice.clone(), prompt);
        }
        model.synthesize_voice_clone_streaming(
            &request.text,
            &prompts[&request.voice],
            language,
            options,
        )
    } else if let Some(style) = &request.style {
        model.synthesize_styled_streaming(
            &request.text,
            style,
            request.voice.parse::<Speaker>().map_err(inference_error)?,
            language,
            options,
        )
    } else {
        model.synthesize_streaming(
            &request.text,
            request.voice.parse::<Speaker>().map_err(inference_error)?,
            language,
            options,
        )
    }
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
        let Some(audio) = stream
            .next_chunk_with_cancel(|| request.audio.is_closed())
            .map_err(inference_error)?
        else {
            if request.audio.is_closed() {
                return Ok(());
            }
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
