use super::*;
use tts_backends::devices::{self, calibration};
use tts_protocol::Device;
pub(super) async fn prepare(
    resources: &Resources,
    config: &Config,
    progress: &mpsc::Sender<Event>,
    mut prepared: Prepared,
    mut selected: Device,
    candidate: Option<Device>,
) -> anyhow::Result<Prepared> {
    let directory = resources.root().join("alignment/qwen");
    let mut device = if config.alignment_device == Device::Auto {
        Device::Cpu
    } else {
        config.alignment_device
    };
    let key = calibration::key("alignment", tts_backends::alignment::resources::REVISION);
    let cached = (config.alignment_device == Device::Auto)
        .then(|| calibration::cached(resources.root(), "alignment", &key))
        .flatten();
    if let Some(record) = &cached {
        device = record.device;
    }
    if let Err(error) =
        tts_backends::alignment::resources::prepare_on(&directory, progress.clone(), device).await
    {
        let _ = progress
            .send(Event::AlignmentStatus {
                sentence_highlight: false,
                reason: Some(format!("alignment resources unavailable: {error}")),
            })
            .await;
        return Ok(prepared);
    }
    let alignment = tts_backends::alignment::QwenAligner::load_on(directory.clone(), device).await;
    let mut aligner: Arc<dyn Aligner> = match alignment {
        Ok(aligner) => aligner,
        Err(error) if config.alignment_device != Device::Auto && device != Device::Cpu => {
            return Err(error);
        }
        Err(error) if device != Device::Cpu => {
            device = Device::Cpu;
            let _ = progress
                .send(devices::status(
                    "alignment",
                    device,
                    Some(format!("provider initialization failed: {error}")),
                ))
                .await;
            let cpu = async {
                tts_backends::alignment::resources::prepare(&directory, progress.clone()).await?;
                tts_backends::alignment::QwenAligner::load(directory.clone()).await
            }
            .await;
            match cpu {
                Ok(aligner) => aligner,
                Err(error) => {
                    let _ = progress
                        .send(Event::AlignmentStatus {
                            sentence_highlight: false,
                            reason: Some(error.to_string()),
                        })
                        .await;
                    return Ok(prepared);
                }
            }
        }
        Err(error) => {
            let _ = progress
                .send(Event::AlignmentStatus {
                    sentence_highlight: false,
                    reason: Some(error.to_string()),
                })
                .await;
            return Ok(prepared);
        }
    };
    if config.alignment_device == Device::Auto
        && cached.is_none()
        && let Some(candidate) = candidate
    {
        let _ = progress
            .send(devices::status(
                "alignment",
                Device::Cpu,
                Some("calibrating: 3 warmups + 5 measurements".into()),
            ))
            .await;
        let evaluation = async {
            tts_backends::alignment::resources::prepare_on(&directory, progress.clone(), candidate)
                .await?;
            let accelerated =
                tts_backends::alignment::QwenAligner::load_on(directory.clone(), candidate).await?;
            let text = "你好，欢迎使用听书功能。今天我们一起阅读一个故事。";
            let pcm = prepared.backend.synthesize(text, &config.voice).await?;
            let audio = tts_core::alignment::AudioClip {
                sample_rate: pcm.sample_rate,
                channels: pcm.channels,
                blocks: vec![Arc::new(pcm)],
                retention: Vec::new(),
            };
            let speech = tts_core::alignment::SpeechText::from_source(text, 0);
            let cpu = calibration::alignment(&*aligner, &speech, &audio).await?;
            let gpu = calibration::alignment(&*accelerated, &speech, &audio).await?;
            Ok::<_, anyhow::Error>((accelerated, cpu, gpu))
        }
        .await;
        let measured = evaluation.is_ok();
        let reason = match evaluation {
            Ok((accelerated, cpu, gpu)) => {
                if gpu.improves(cpu) {
                    device = candidate;
                    aligner = accelerated;
                }
                format!(
                    "CPU {:.1} ms; {candidate:?} {:.1} ms",
                    cpu.total_ms, gpu.total_ms
                )
            }
            Err(error) => format!("accelerator calibration failed: {error}"),
        };
        if measured {
            calibration::save(
                resources.root(),
                "alignment",
                &calibration::Record {
                    key,
                    device,
                    reason: reason.clone(),
                },
            )?;
        }
        let _ = progress
            .send(devices::status("alignment", device, Some(reason)))
            .await;
    } else {
        let _ = progress
            .send(devices::status(
                "alignment",
                device,
                cached.map(|r| r.reason),
            ))
            .await;
    }
    if config.tts_device == Device::Auto
        && config.alignment_device == Device::Auto
        && (selected != Device::Cpu || device != Device::Cpu)
    {
        super::concurrency::calibrate(
            resources,
            config,
            progress,
            &mut prepared,
            &mut selected,
            &mut device,
            &mut aligner,
        )
        .await?;
    }
    if config.alignment_device == Device::Auto && device != Device::Cpu {
        aligner = tts_backends::alignment::QwenAligner::load_with_recovery(
            directory.clone(),
            device,
            Some(progress.clone()),
        )
        .await?;
    }
    prepared.aligner = Some(aligner);

    Ok(prepared)
}
