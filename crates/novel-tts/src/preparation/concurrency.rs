use super::*;
use tts_backends::devices::{self, calibration};
use tts_protocol::Device;
pub(super) async fn calibrate(
    resources: &Resources,
    config: &Config,
    progress: &mpsc::Sender<Event>,
    prepared: &mut Prepared,
    selected: &mut Device,
    device: &mut Device,
    aligner: &mut Arc<dyn Aligner>,
) -> anyhow::Result<()> {
    if !resources
        .available_devices_for(&config.backend, config.model.as_deref())
        .contains(&Device::Cpu)
    {
        let _ = progress.send(resources.device_status_for(&config.backend, config.model.as_deref(), *selected, Some("this model has no accepted CPU adapter; concurrent CPU comparison is unavailable".into()))).await;
        return Ok(());
    }
    let directory = resources.root().join("alignment/qwen");
    let mut available = resources.available_devices_for(&config.backend, config.model.as_deref());
    available.extend(devices::available());
    let joint_key = calibration::key(
        "concurrent",
        &format!(
            "{}:{}:{}:{selected:?}:{device:?}",
            config.model.as_deref().unwrap_or("legacy"),
            super::synthesis::tts_revision(config),
            tts_backends::alignment::resources::REVISION
        ),
    );
    if calibration::cached_for(resources.root(), "concurrent", &joint_key, &available).is_none() {
        let _ = progress
            .send(devices::status(
                "concurrent",
                Device::Cpu,
                Some("calibrating concurrent synthesis and alignment".into()),
            ))
            .await;
        let cpu_backend = resources
            .prepare_model_on(
                &config.backend,
                config.model.as_deref(),
                progress.clone(),
                Device::Cpu,
            )
            .await?;
        tts_backends::alignment::resources::prepare(&directory, progress.clone()).await?;
        let cpu_aligner =
            tts_backends::alignment::QwenAligner::load(directory.to_path_buf()).await?;
        let text = "你好，欢迎使用听书功能。今天我们一起阅读一个故事。";
        let pcm = cpu_backend.synthesize(text, &config.voice).await?;
        let audio = tts_core::alignment::AudioClip {
            sample_rate: pcm.sample_rate,
            channels: pcm.channels,
            blocks: vec![Arc::new(pcm)],
            retention: Vec::new(),
        };
        let speech = tts_core::alignment::SpeechText::from_source(text, 0);
        let cpu =
            calibration::concurrent(&*cpu_backend, &config.voice, &*cpu_aligner, &speech, &audio)
                .await?;
        let accelerated = calibration::concurrent(
            &*prepared.backend,
            &config.voice,
            &**aligner,
            &speech,
            &audio,
        )
        .await;
        let accepted = accelerated.as_ref().is_ok_and(|value| value.improves(cpu));
        let reason = match &accelerated {
            Ok(measured) => format!(
                "concurrent CPU {:.1} ms / first {:.1} ms; selected {:.1} ms / first {:.1} ms; qualifies {accepted}",
                cpu.total_ms, cpu.first_ms, measured.total_ms, measured.first_ms
            ),
            Err(error) => format!("concurrent calibration failed: {error}"),
        };
        if !accepted {
            prepared.backend = cpu_backend;
            *aligner = cpu_aligner;
            *selected = Device::Cpu;
            *device = Device::Cpu;
        }
        // Cpu marks a measured rejection; the accelerated device marks acceptance.
        if accelerated.is_ok() {
            calibration::save(
                resources.root(),
                "concurrent",
                &calibration::Record {
                    key: joint_key,
                    device: if accepted {
                        if *selected != Device::Cpu {
                            *selected
                        } else {
                            *device
                        }
                    } else {
                        Device::Cpu
                    },
                    reason: reason.clone(),
                },
            )?;
        }
        let _ = progress
            .send(resources.device_status_for(
                &config.backend,
                config.model.as_deref(),
                *selected,
                Some(reason.clone()),
            ))
            .await;
        let _ = progress
            .send(devices::status("alignment", *device, Some(reason)))
            .await;
    } else if calibration::cached_for(resources.root(), "concurrent", &joint_key, &available)
        .is_some_and(|r| r.device == Device::Cpu)
    {
        prepared.backend = resources
            .prepare_model_on(
                &config.backend,
                config.model.as_deref(),
                progress.clone(),
                Device::Cpu,
            )
            .await?;
        tts_backends::alignment::resources::prepare(&directory, progress.clone()).await?;
        *aligner = tts_backends::alignment::QwenAligner::load(directory.to_path_buf()).await?;
        *selected = Device::Cpu;
        *device = Device::Cpu;
        let _ = progress
            .send(resources.device_status_for(
                &config.backend,
                config.model.as_deref(),
                Device::Cpu,
                Some("concurrent calibration rejected accelerator pair".into()),
            ))
            .await;
        let _ = progress
            .send(devices::status(
                "alignment",
                Device::Cpu,
                Some("concurrent calibration rejected accelerator pair".into()),
            ))
            .await;
    }

    Ok(())
}
