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
    reference_model(
        directory,
        super::models::Model::Q8,
        device,
        text,
        description,
        output,
    )
    .await
}
pub async fn reference_model(
    directory: PathBuf,
    variant: super::models::Model,
    device: Device,
    text: String,
    description: String,
    output: PathBuf,
) -> anyhow::Result<()> {
    anyhow::ensure!(
        variant.compiled_devices().contains(&device),
        "unsupported VoxCPM2 model device"
    );
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
    let thread = std::thread::Builder::new()
        .name("voxcpm-design".into())
        .spawn(move || {
            let generate = || -> anyhow::Result<()> {
                let device = super::runtime::candle_device(device)?;
                let mut model = variant.load(&directory, &device, &|| result.is_closed())?;
                if result.is_closed() {
                    anyhow::bail!("voice design cancelled");
                }
                let mut samples = Vec::new();
                let outcome = model.generate(
                    &format!("({description}){text}"),
                    None,
                    &voxcpm::Options::default(),
                    &|| result.is_closed(),
                    |pcm| {
                        if result.is_closed() {
                            return Ok(());
                        }
                        samples.extend_from_slice(pcm);
                        Ok(())
                    },
                )?;
                anyhow::ensure!(
                    !result.is_closed()
                        && outcome == voxcpm::Outcome::Eos
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
    let mut job = DesignJob {
        receiver: received,
        thread: Some(thread),
    };
    (&mut job.receiver)
        .await
        .map_err(|_| anyhow::anyhow!("Vox design thread exited"))?
}

// Closing the receiver makes both weight loading and generation cancellable before join.
struct DesignJob {
    receiver: tokio::sync::oneshot::Receiver<anyhow::Result<()>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Drop for DesignJob {
    fn drop(&mut self) {
        self.receiver.close();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
