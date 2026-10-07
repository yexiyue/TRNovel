use super::{Variant, model::Model, voice_store};
use std::path::PathBuf;
use tokio::sync::{mpsc, oneshot};
use tts_core::backend::{AudioChunk, BackendError, Pcm};
pub(super) struct Request {
    pub text: String,
    pub voice: String,
    pub audio: mpsc::Sender<Result<AudioChunk, BackendError>>,
}
pub(super) fn run(
    directory: PathBuf,
    variant: Variant,
    mut jobs: mpsc::Receiver<Request>,
    ready: oneshot::Sender<Result<(), BackendError>>,
) {
    let mut model = match Model::load(&directory) {
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
        if let Err(error) = generate(&mut model, &directory, variant, &request)
            && !request.audio.is_closed()
        {
            let _ = request
                .audio
                .blocking_send(Err(BackendError::Synthesis(error.to_string())));
        }
    }
}
fn generate(
    model: &mut Model,
    directory: &std::path::Path,
    variant: Variant,
    request: &Request,
) -> anyhow::Result<()> {
    let store = voice_store(directory, variant)?;
    let record = store.load(&request.voice)?;
    let path = store.path(&request.voice)?;
    let cache = path.join("features.json");
    let prompt = if cache.exists() {
        let prompt: super::model::ReferencePrompt =
            serde_json::from_reader(std::fs::File::open(&cache)?)?;
        anyhow::ensure!(
            prompt.transcript == record.transcript,
            "reference transcript/cache mismatch"
        );
        prompt
    } else {
        let samples = crate::reference::load(&path.join("reference.wav"), 24000)?;
        let prompt = model.reference_prompt(&samples, &record.transcript)?;
        let temporary = tempfile::NamedTempFile::new_in(&path)?;
        serde_json::to_writer(temporary.as_file(), &prompt)?;
        temporary.persist(cache)?;
        prompt
    };
    let started = std::time::Instant::now();
    let Some(samples) =
        model.generate_with_prompt(&request.text, &prompt, || request.audio.is_closed())?
    else {
        return Ok(());
    };
    if request.audio.is_closed() {
        return Ok(());
    }
    let pcm = Pcm {
        samples,
        sample_rate: 24000,
        channels: 1,
    };
    let audio_ms = pcm.duration_ms()?;
    if std::env::var("NOVEL_TTS_DIAGNOSTICS").is_ok_and(|v| v == "1") {
        eprintln!(
            "zipvoice generation_ms={} audio_ms={audio_ms}",
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
