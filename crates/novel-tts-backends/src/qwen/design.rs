//! VoiceDesign creates one reference recording; Base is used for subsequent narration.
use super::{MAX_FRAMES, runtime};
use qwen3_tts::{AudioBuffer, Language, SynthesisOptions};
use std::path::PathBuf;
use tts_protocol::Device;

pub async fn reference(
    directory: PathBuf,
    device: Device,
    text: String,
    description: String,
    output: PathBuf,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        !description.trim().is_empty() && description.len() <= 4096,
        "provide a voice description up to 4096 bytes"
    );
    anyhow::ensure!(
        !text.trim().is_empty() && text.chars().count() <= 100,
        "design reference text must contain 1..100 characters"
    );
    anyhow::ensure!(
        super::models::Model::detect(&directory)? == super::models::Model::Design17,
        "voice design requires 1.7b-voicedesign weights"
    );
    let (completed, receiver) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("qwen-voice-design".into())
        .spawn(move || {
            let result = (|| -> anyhow::Result<()> {
                let model = runtime::load(&directory, device)?;
                let mut stream = model.synthesize_voice_design_streaming(
                    &text,
                    &description,
                    Language::Chinese,
                    SynthesisOptions {
                        max_length: MAX_FRAMES,
                        chunk_frames: 20,
                        seed: Some(42),
                        ..Default::default()
                    },
                )?;
                let mut samples = Vec::new();
                while let Some(chunk) = stream.next_chunk_with_cancel(|| completed.is_closed())? {
                    samples.extend(chunk.samples);
                }
                anyhow::ensure!(!completed.is_closed(), "voice design cancelled");
                super::runtime::validate_completion(stream.is_done(), !samples.is_empty())?;
                let audio = AudioBuffer::new(samples, 24000);
                super::validate_reference(&audio)?;
                audio.save(output)?;
                Ok(())
            })();
            // GPU weights are released before the caller can load Base.
            let _ = completed.send(result);
        })?;
    receiver
        .await
        .map_err(|_| anyhow::anyhow!("voice design thread exited"))?
}
