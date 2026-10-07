use super::*;
use tts_backends::devices::calibration;
use tts_protocol::Device;
pub(super) async fn prepare(
    resources: &Resources,
    config: &Config,
    progress: &mpsc::Sender<Event>,
) -> anyhow::Result<(Rc<dyn Backend>, Device, Option<Device>)> {
    let candidate = resources
        .available_devices_for(&config.backend, config.model.as_deref())
        .into_iter()
        .find(|d| *d != Device::Cpu);
    let mut selected = if config.tts_device == Device::Auto {
        resources
            .available_devices_for(&config.backend, config.model.as_deref())
            .into_iter()
            .find(|device| *device == Device::Cpu)
            .or(candidate)
            .ok_or_else(|| anyhow::anyhow!("no available device for this model"))?
    } else {
        config.tts_device
    };
    let key = calibration::key(
        "tts",
        &format!(
            "{}:{}:{}",
            config.backend,
            config.model.as_deref().unwrap_or("legacy"),
            tts_revision(config)
        ),
    );
    let cached = (config.tts_device == Device::Auto)
        .then(|| {
            calibration::cached_for(
                resources.root(),
                "tts",
                &key,
                &resources.available_devices_for(&config.backend, config.model.as_deref()),
            )
        })
        .flatten();
    if let Some(record) = &cached {
        selected = record.device;
    }
    let mut backend = match resources
        .prepare_model_on(
            &config.backend,
            config.model.as_deref(),
            progress.clone(),
            selected,
        )
        .await
    {
        Ok(backend) => backend,
        Err(error)
            if config.tts_device == Device::Auto
                && selected != Device::Cpu
                && resources
                    .available_devices_for(&config.backend, config.model.as_deref())
                    .contains(&Device::Cpu) =>
        {
            selected = Device::Cpu;
            let _ = progress
                .send(resources.device_status_for(
                    &config.backend,
                    config.model.as_deref(),
                    selected,
                    Some(format!("provider initialization failed: {error}")),
                ))
                .await;
            resources
                .prepare_model_on(
                    &config.backend,
                    config.model.as_deref(),
                    progress.clone(),
                    Device::Cpu,
                )
                .await?
        }
        Err(error) => return Err(error),
    };
    if config.tts_device == Device::Auto
        && cached.is_none()
        && resources
            .available_devices_for(&config.backend, config.model.as_deref())
            .contains(&Device::Cpu)
        && matches!(
            config.backend.as_str(),
            "moss" | "qwen" | "voxcpm" | "omnivoice"
        )
        && let Some(device) = candidate
    {
        let _ = progress
            .send(resources.device_status_for(
                &config.backend,
                config.model.as_deref(),
                Device::Cpu,
                Some("calibrating: 3 warmups + 5 measurements".into()),
            ))
            .await;
        let evaluation = async {
            let accelerated = resources
                .prepare_model_on(
                    &config.backend,
                    config.model.as_deref(),
                    progress.clone(),
                    device,
                )
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
            .send(resources.device_status_for(
                &config.backend,
                config.model.as_deref(),
                selected,
                Some(reason),
            ))
            .await;
    } else {
        let _ = progress
            .send(resources.device_status_for(
                &config.backend,
                config.model.as_deref(),
                selected,
                cached.map(|r| r.reason),
            ))
            .await;
    }
    #[cfg(feature = "moss")]
    if config.tts_device == Device::Auto
        && selected != Device::Cpu
        && config.backend == "moss"
        && config.model.as_deref().is_none_or(|id| id == "nano")
    {
        drop(backend);
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
        drop(backend);
        backend = Rc::new(
            tts_backends::qwen::QwenBackend::load_with_recovery(
                tts_backends::qwen::models::Model::parse(config.model.as_deref())?
                    .directory(resources.root()),
                selected,
                Some(progress.clone()),
            )
            .await?,
        );
    }

    Ok((backend, selected, candidate))
}
pub(super) fn tts_revision(config: &Config) -> &'static str {
    let backend = config.backend.as_str();
    #[cfg(feature = "voxcpm")]
    if backend == "voxcpm" {
        return tts_backends::voxcpm::resources::REVISION;
    }
    #[cfg(feature = "omnivoice")]
    if backend == "omnivoice" {
        return tts_backends::omnivoice::resources::REVISION;
    }
    #[cfg(feature = "moss-candle")]
    if backend == "moss"
        && let Some(model) = config.model.as_deref().filter(|id| *id != "nano")
    {
        return tts_backends::moss::candle::Mode::parse(model)
            .expect("validated model")
            .revision();
    }
    #[cfg(feature = "moss")]
    if backend == "moss" {
        return tts_backends::moss::resources::REVISION;
    }
    #[cfg(feature = "qwen")]
    if backend == "qwen" {
        return tts_backends::qwen::models::Model::parse(config.model.as_deref())
            .expect("validated model")
            .revision();
    }
    let _ = backend;
    "unavailable"
}
