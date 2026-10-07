use super::voice_store;
use ::omnivoice::{
    contracts::{
        GenerationRequest, I64Tensor2, ReferenceAudioInput, VoiceClonePrompt, WaveformInput,
    },
    pipeline::Pipeline,
    runtime::{DeviceSpec, RuntimeOptions},
};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::{mpsc, oneshot};
use tts_core::backend::{AudioChunk, BackendError, Pcm};
use tts_protocol::Device;

pub(super) struct Request {
    text: String,
    voice: String,
    audio: mpsc::Sender<Result<AudioChunk, BackendError>>,
}
impl Request {
    pub(super) fn new(
        text: &str,
        voice: &str,
        audio: mpsc::Sender<Result<AudioChunk, BackendError>>,
    ) -> Self {
        Self {
            text: text.into(),
            voice: voice.into(),
            audio,
        }
    }
}
pub(super) fn options_device(device: Device) -> Option<DeviceSpec> {
    match device {
        Device::Cpu => Some(DeviceSpec::Cpu),
        Device::Cuda => Some(DeviceSpec::Cuda(0)),
        Device::Metal => Some(DeviceSpec::Metal),
        _ => None,
    }
}
pub(super) fn run(
    directory: PathBuf,
    device: Device,
    mut jobs: mpsc::Receiver<Request>,
    ready: oneshot::Sender<Result<(), BackendError>>,
) {
    let pipeline = match Pipeline::from_options(
        RuntimeOptions::new(&directory)
            .with_device(options_device(device).expect("validated device"))
            .with_seed(42),
    ) {
        Ok(model) => model,
        Err(error) => {
            let _ = ready.send(Err(BackendError::Initialize(error.to_string())));
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
        let audio = request.audio.clone();
        pipeline.set_cancellation_probe(Some(::omnivoice::stage0_model::CancellationProbe(
            Arc::new(move || audio.is_closed()),
        )));
        if let Err(error) = generate(&pipeline, &directory, &request)
            && !request.audio.is_closed()
        {
            let _ = request.audio.blocking_send(Err(error));
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedPrompt {
    dims: (usize, usize),
    tokens: Vec<i64>,
    transcript: String,
    source_transcript: String,
    rms: Option<f32>,
}
fn prompt(pipeline: &Pipeline, directory: &Path, voice: &str) -> anyhow::Result<VoiceClonePrompt> {
    let store = voice_store(directory)?;
    let record = store.load(voice)?;
    let path = store.path(voice)?;
    let cache = path.join("prompt.json");
    if cache.exists() {
        let cached: CachedPrompt = serde_json::from_reader(std::fs::File::open(&cache)?)?;
        anyhow::ensure!(
            cached.dims.0 == 8
                && (1..=750).contains(&cached.dims.1)
                && cached.source_transcript == record.transcript
                && cached.tokens.iter().all(|v| (0..1024).contains(v))
                && cached.rms.is_none_or(|r| r.is_finite() && r > 0.0),
            "invalid Omni reference cache; re-import {voice}"
        );
        return Ok(VoiceClonePrompt {
            ref_audio_tokens: I64Tensor2::new(cached.dims, cached.tokens)?,
            ref_text: cached.transcript,
            ref_rms: cached.rms,
        });
    }
    let samples = crate::reference::load(&path.join("reference.wav"), 24000)?;
    let prompt = pipeline.create_voice_clone_prompt_from_audio(
        &ReferenceAudioInput::Waveform(WaveformInput::mono(samples, 24000)),
        Some(&record.transcript),
        true,
        None,
    )?;
    let cached = CachedPrompt {
        dims: prompt.ref_audio_tokens.dims(),
        tokens: prompt.ref_audio_tokens.data.clone(),
        transcript: prompt.ref_text.clone(),
        source_transcript: record.transcript,
        rms: prompt.ref_rms,
    };
    let temporary = tempfile::NamedTempFile::new_in(&path)?;
    serde_json::to_writer(temporary.as_file(), &cached)?;
    temporary.persist(cache)?;
    Ok(prompt)
}
fn generate(pipeline: &Pipeline, directory: &Path, request: &Request) -> Result<(), BackendError> {
    let started = std::time::Instant::now();
    let mut input = GenerationRequest::new_text_only(&request.text).with_language("zh");
    if request.voice.starts_with("custom:") {
        input = input.with_voice_clone_prompt(
            prompt(pipeline, directory, &request.voice)
                .map_err(|e| BackendError::Unsupported(e.to_string()))?,
        );
    }
    if request.audio.is_closed() {
        return Ok(());
    }
    let mut result = pipeline
        .generate(&input)
        .map_err(|e| BackendError::Synthesis(e.to_string()))?;
    if request.audio.is_closed() {
        return Ok(());
    }
    if result.len() != 1 {
        return Err(BackendError::Synthesis(
            "Omni returned an invalid audio batch".into(),
        ));
    }
    let audio = result.remove(0);
    let pcm = Pcm {
        samples: audio.samples,
        sample_rate: audio.sample_rate,
        channels: 1,
    };
    let audio_ms = pcm.duration_ms()?;
    if std::env::var("NOVEL_TTS_DIAGNOSTICS").is_ok_and(|v| v == "1") {
        eprintln!(
            "omnivoice generation_ms={} audio_ms={audio_ms}",
            started.elapsed().as_millis()
        );
    }
    if request
        .audio
        .blocking_send(Ok(AudioChunk::Pcm(pcm)))
        .is_ok()
    {
        let _ = request.audio.blocking_send(Ok(AudioChunk::End));
    }
    Ok(())
}
