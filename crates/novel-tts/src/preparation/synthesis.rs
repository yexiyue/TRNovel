use super::*;
use tts_backends::devices::calibration;
use tts_protocol::Device;
pub(super) async fn prepare(
    resources: &Resources,
    config: &Config,
    progress: &mpsc::Sender<Event>,
) -> anyhow::Result<(Rc<dyn Backend>, Device, Option<Device>)> {
    let candidate = resources
        .available_devices(&config.backend)
        .into_iter()
        .find(|d| *d != Device::Cpu);
    let mut selected = if config.tts_device == Device::Auto {
        Device::Cpu
    } else {
        config.tts_device
    };
    let key = calibration::key(
        "tts",
        &format!("{}:{}", config.backend, tts_revision(&config.backend)),
    );
    let cached = (config.tts_device == Device::Auto)
        .then(|| {
            calibration::cached_for(
                resources.root(),
                "tts",
                &key,
                &resources.available_devices(&config.backend),
            )
        })
        .flatten();
    if let Some(record) = &cached {
        selected = record.device;
    }
    let mut backend = match resources
        .prepare_on(&config.backend, progress.clone(), selected)
        .await
    {
        Ok(backend) => backend,
        Err(error) if config.tts_device == Device::Auto && selected != Device::Cpu => {
            selected = Device::Cpu;
            let _ = progress
                .send(resources.device_status(
                    &config.backend,
                    selected,
                    Some(format!("provider initialization failed: {error}")),
                ))
                .await;
            resources.prepare(&config.backend, progress.clone()).await?
        }
        Err(error) => return Err(error),
    };
    if config.tts_device == Device::Auto
        && cached.is_none()
        && (config.backend == "moss" || config.backend == "qwen")
        && let Some(device) = candidate
    {
        let _ = progress
            .send(resources.device_status(
                &config.backend,
                Device::Cpu,
                Some("calibrating: 3 warmups + 5 measurements".into()),
            ))
            .await;
        let evaluation = async {
            let accelerated = resources
                .prepare_on(&config.backend, progress.clone(), device)
                .await?;
            let cpu = calibration::synthesis(&*backend, &config.voice).await?;
            let gpu = calibration::synthesis(&*accelerated, &config.voice).await?;
            Ok::<_, anyhow::Error>((accelerated, cpu, gpu))
        }
        .await;
        let measured = evaluation.is_ok();
        let reason = match evaluation {
            Ok((accelerated, cpu, gpu)) => {
                if gpu.improves(cpu) {
                    selected = device;
                    backend = accelerated;
                }
                format!(
                    "CPU {:.1} ms / first {:.1} ms; {device:?} {:.1} ms / first {:.1} ms",
                    cpu.total_ms, cpu.first_ms, gpu.total_ms, gpu.first_ms
                )
            }
            Err(error) => format!("accelerator calibration failed: {error}"),
        };
        if measured {
            calibration::save(
                resources.root(),
                "tts",
                &calibration::Record {
                    key,
                    device: selected,
                    reason: reason.clone(),
                },
            )?;
        }
        let _ = progress
            .send(resources.device_status(&config.backend, selected, Some(reason)))
            .await;
    } else {
        let _ = progress
            .send(resources.device_status(&config.backend, selected, cached.map(|r| r.reason)))
            .await;
    }
    #[cfg(feature = "moss")]
    if config.tts_device == Device::Auto && selected != Device::Cpu && config.backend == "moss" {
        backend = Rc::new(
            tts_backends::moss::MossBackend::load_with_recovery(
                resources.root().join("moss"),
                selected,
                Some(progress.clone()),
            )
            .await?,
        );
    }
    #[cfg(feature = "qwen")]
    if config.tts_device == Device::Auto && selected != Device::Cpu && config.backend == "qwen" {
        backend = Rc::new(
            tts_backends::qwen::QwenBackend::load_with_recovery(
                resources.root().join("qwen"),
                selected,
                Some(progress.clone()),
            )
            .await?,
        );
    }

    Ok((backend, selected, candidate))
}
pub(super) fn tts_revision(backend: &str) -> &'static str {
    #[cfg(feature = "moss")]
    if backend == "moss" {
        return tts_backends::moss::resources::REVISION;
    }
    #[cfg(feature = "qwen")]
    if backend == "qwen" {
        return tts_backends::qwen::resources::REVISION;
    }
    let _ = backend;
    "kokoro-v1.1-zh"
}
