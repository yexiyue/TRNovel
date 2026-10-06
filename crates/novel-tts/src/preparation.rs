//! Composition and calibration belong to the program, not the model-free core.
use crate::resources::Resources;
use std::{rc::Rc, sync::Arc};
use tokio::sync::mpsc;
use tts_core::{alignment::Aligner, backend::Backend};
use tts_protocol::{Config, Event};
pub struct Prepared {
    pub backend: Rc<dyn Backend>,
    pub aligner: Option<Arc<dyn Aligner>>,
}

#[cfg(all(feature = "alignment", any(feature = "moss", feature = "kokoro")))]
mod alignment;
#[cfg(all(feature = "alignment", any(feature = "moss", feature = "kokoro")))]
mod concurrency;
#[cfg(any(feature = "moss", feature = "kokoro"))]
mod synthesis;

#[cfg(any(feature = "moss", feature = "kokoro"))]
pub fn validate_device(component: &str, device: tts_protocol::Device) -> anyhow::Result<()> {
    if component == "alignment" && !cfg!(feature = "alignment") {
        anyhow::ensure!(
            device == tts_protocol::Device::Auto,
            "alignment feature is not compiled"
        );
        return Ok(());
    }
    tts_backends::devices::validate(device)
}

#[cfg(any(feature = "moss", feature = "kokoro"))]
pub fn unprepared_device_status(component: &str) -> Event {
    if component == "alignment" && !cfg!(feature = "alignment") {
        return Event::DeviceStatus {
            component: component.into(),
            compiled: vec![],
            available: vec![],
            selected: tts_protocol::Device::Auto,
            reason: Some("alignment feature is not compiled".into()),
        };
    }
    tts_backends::devices::status(
        component,
        tts_protocol::Device::Auto,
        Some("resources not prepared".into()),
    )
}

pub fn validate_alignment_enabled(enabled: bool) -> anyhow::Result<()> {
    anyhow::ensure!(
        !enabled || cfg!(feature = "alignment"),
        "alignment feature is not compiled"
    );
    Ok(())
}

pub async fn prepare(
    resources: Resources,
    config: Config,
    progress: mpsc::Sender<Event>,
) -> anyhow::Result<Prepared> {
    #[cfg(not(any(feature = "moss", feature = "kokoro")))]
    {
        let _ = (resources, config, progress);
        anyhow::bail!("no synthesis backend is compiled");
    }
    #[cfg(any(feature = "moss", feature = "kokoro"))]
    {
        validate_device("tts", config.tts_device)?;
        validate_alignment_enabled(config.alignment_enabled)?;
        if config.alignment_enabled {
            validate_device("alignment", config.alignment_device)?;
        }
        let (backend, selected, candidate) =
            synthesis::prepare(&resources, &config, &progress).await?;
        let prepared = Prepared {
            backend,
            aligner: None,
        };
        #[cfg(feature = "alignment")]
        let prepared = if config.alignment_enabled {
            alignment::prepare(
                &resources, &config, &progress, prepared, selected, candidate,
            )
            .await?
        } else {
            prepared
        };
        #[cfg(not(feature = "alignment"))]
        let _ = (selected, candidate);
        if !config.alignment_enabled {
            let _ = progress
                .send(Event::AlignmentStatus {
                    sentence_highlight: false,
                    reason: Some("逐句对齐已关闭".into()),
                })
                .await;
        }
        let _ = progress
            .send(Event::ResourceState {
                stage: "模型就绪".into(),
                resource: config.backend.clone(),
            })
            .await;
        Ok(prepared)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn disabled_alignment_is_valid_without_compiled_adapter() {
        assert!(validate_alignment_enabled(false).is_ok());
        assert_eq!(
            validate_alignment_enabled(true).is_ok(),
            cfg!(feature = "alignment")
        );
    }
    #[cfg(all(unix, feature = "moss"))]
    #[tokio::test]
    async fn real_disabled_alignment_does_not_prepare_any_alignment_resources() {
        let Some(model) = std::env::var_os("TRNOVEL_MOSS_MODEL_DIR") else {
            return;
        };
        let root = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(model, root.path().join("moss")).unwrap();
        let (progress, mut events) = mpsc::channel(128);
        let prepared = prepare(
            Resources::new(Some(root.path().into())).unwrap(),
            Config {
                tts_device: tts_protocol::Device::Cpu,
                alignment_device: tts_protocol::Device::Cuda,
                alignment_enabled: false,
                ..Default::default()
            },
            progress,
        )
        .await
        .unwrap();
        assert!(prepared.aligner.is_none());
        assert!(!root.path().join("alignment").exists());
        while let Ok(event) = events.try_recv() {
            if let Event::ResourceState { resource, .. } | Event::ModelProgress { resource, .. } =
                event
            {
                assert!(!resource.contains("qwen") && !resource.contains("alignment"));
            }
        }
    }
}
