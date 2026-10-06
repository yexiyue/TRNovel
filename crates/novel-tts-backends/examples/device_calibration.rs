//! Real synthesis calibration on one model/voice; no audio output device.
#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let directory = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("model directory required"))?;
    let mut baseline = None;
    for device in [tts_protocol::Device::Cpu, tts_protocol::Device::Coreml] {
        let backend =
            novel_tts_backends::moss::MossBackend::load_on(directory.clone().into(), device)
                .await?;
        let measured =
            novel_tts_backends::devices::calibration::synthesis(&backend, "Weiguo").await?;
        eprintln!("{device:?}: {measured:?}");
        if let Some(cpu) = baseline {
            eprintln!("qualifies: {}", measured.improves(cpu));
        } else {
            baseline = Some(measured);
        }
    }
    Ok(())
}
