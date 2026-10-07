//! Design once, save the spoken reference, then narrate with its clone cache.
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
        !text.trim().is_empty() && text.chars().count() <= 100,
        "design reference requires 1..100 characters"
    );
    anyhow::ensure!(
        !description.trim().is_empty()
            && description.chars().count() <= 200
            && !description.contains(['(', ')', '（', '）', '\0']),
        "description requires 1..200 characters without parentheses or NUL"
    );
    let (result, received) = tokio::sync::oneshot::channel();
    std::thread::Builder::new()
        .name("voxcpm-design".into())
        .spawn(move || {
            let generate = || -> anyhow::Result<()> {
                let device = super::runtime::native_device(device)
                    .ok_or_else(|| anyhow::anyhow!("unavailable Vox design device"))?;
                let mut model = voxcpm_sys::Model::load(
                    &directory.join("VoxCPM2-BaseLM-Q8_0.gguf"),
                    &directory.join("VoxCPM2-Acoustic-F16.gguf"),
                    device,
                )
                .map_err(anyhow::Error::msg)?;
                if result.is_closed() {
                    anyhow::bail!("voice design cancelled");
                }
                let mut samples = Vec::new();
                let outcome = model
                    .generate(&format!("({description}){text}"), None, 200, |pcm| {
                        if result.is_closed() {
                            return false;
                        }
                        samples.extend_from_slice(pcm);
                        true
                    })
                    .map_err(anyhow::Error::msg)?;
                anyhow::ensure!(
                    outcome == voxcpm_sys::Outcome::Complete
                        && (48000..=48000 * 30).contains(&samples.len())
                        && samples.iter().all(|s| s.is_finite()),
                    "voice design cancelled, truncated or invalid"
                );
                let mut wav = hound::WavWriter::create(
                    output,
                    hound::WavSpec {
                        channels: 1,
                        sample_rate: 48000,
                        bits_per_sample: 32,
                        sample_format: hound::SampleFormat::Float,
                    },
                )?;
                for sample in samples {
                    wav.write_sample(sample)?;
                }
                wav.finalize()?;
                Ok(())
            };
            let outcome = generate();
            let _ = result.send(outcome);
        })?;
    received
        .await
        .map_err(|_| anyhow::anyhow!("Vox design thread exited"))?
}
