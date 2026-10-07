//! Create one tagged reference; subsequent narration uses its cached clone prompt.
use std::{path::PathBuf, sync::Arc};
use tts_protocol::Device;
pub async fn reference(
    directory: PathBuf,
    device: Device,
    text: String,
    description: String,
    output: PathBuf,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        !text.trim().is_empty() && text.chars().count() <= 100,
        "design reference must contain 1..100 characters"
    );
    anyhow::ensure!(
        !description.trim().is_empty() && description.len() <= 4096,
        "provide a voice description up to 4096 bytes"
    );
    let (completed, mut receiver) = tokio::sync::oneshot::channel::<()>();
    let completed = Arc::new(completed);
    // Receiver drop is observed directly at diffusion and decode boundaries.
    let (result, received) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("omni-voice-design".into())
        .spawn(move || {
            let generated = (|| -> anyhow::Result<()> {
                let pipeline = ::omnivoice::pipeline::Pipeline::from_options(
                    ::omnivoice::runtime::RuntimeOptions::new(directory)
                        .with_device(
                            super::runtime::options_device(device)
                                .ok_or_else(|| anyhow::anyhow!("unsupported Omni device"))?,
                        )
                        .with_seed(42),
                )?;
                let signal = completed.clone();
                pipeline.set_cancellation_probe(Some(
                    ::omnivoice::stage0_model::CancellationProbe(Arc::new(move || {
                        signal.is_closed()
                    })),
                ));
                let request = ::omnivoice::contracts::GenerationRequest::new_text_only(text)
                    .with_language("zh")
                    .with_instruct(description);
                let mut generated = pipeline.generate(&request)?;
                anyhow::ensure!(
                    !completed.is_closed() && generated.len() == 1,
                    "voice design cancelled or returned an invalid batch"
                );
                let audio = generated.remove(0);
                let pcm = tts_core::backend::Pcm {
                    samples: audio.samples,
                    sample_rate: audio.sample_rate,
                    channels: 1,
                };
                pcm.duration_ms()?;
                let mut wav = hound::WavWriter::create(
                    output,
                    hound::WavSpec {
                        channels: 1,
                        sample_rate: pcm.sample_rate,
                        bits_per_sample: 32,
                        sample_format: hound::SampleFormat::Float,
                    },
                )?;
                for sample in pcm.samples {
                    wav.write_sample(sample)?;
                }
                wav.finalize()?;
                Ok(())
            })();
            let _ = result.send(generated);
        })?;
    let result = received
        .await
        .map_err(|_| anyhow::anyhow!("Omni voice design thread exited"));
    receiver.close();
    result?
}
