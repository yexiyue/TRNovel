//! Provider selection is explicit; registration never implies measured acceleration.
pub mod calibration;
#[cfg(any(feature = "coreml", feature = "ort-cuda"))]
use ort::execution_providers::ExecutionProvider;
#[cfg(any(feature = "moss", feature = "alignment"))]
use ort::session::Session;
#[cfg(any(feature = "moss", feature = "alignment"))]
use std::path::Path;
use tts_protocol::{Device, Event};

pub fn compiled() -> Vec<Device> {
    vec![
        Device::Cpu,
        #[cfg(feature = "coreml")]
        Device::Coreml,
        #[cfg(feature = "ort-cuda")]
        Device::Cuda,
    ]
}
pub fn available() -> Vec<Device> {
    compiled()
        .into_iter()
        .filter(|device| match device {
            Device::Cpu => true,
            #[cfg(feature = "coreml")]
            Device::Coreml => ort::execution_providers::CoreMLExecutionProvider::default()
                .is_available()
                .unwrap_or(false),
            #[cfg(feature = "ort-cuda")]
            Device::Cuda => {
                ort::execution_providers::CUDAExecutionProvider::default()
                    .is_available()
                    .unwrap_or(false)
                    && std::process::Command::new("nvidia-smi")
                        .args(["--query-gpu=uuid", "--format=csv,noheader"])
                        .output()
                        .is_ok_and(|output| output.status.success() && !output.stdout.is_empty())
            }
            _ => false,
        })
        .collect()
}
pub fn validate(device: Device) -> anyhow::Result<()> {
    anyhow::ensure!(
        device == Device::Auto || available().contains(&device),
        "device {device:?} is unavailable; compiled {:?}, available {:?}",
        compiled(),
        available()
    );
    Ok(())
}
pub fn status(component: &str, selected: Device, reason: Option<String>) -> Event {
    Event::DeviceStatus {
        component: component.into(),
        compiled: compiled(),
        available: available(),
        selected,
        reason,
    }
}
#[cfg(any(feature = "moss", feature = "alignment"))]
pub fn session(path: &Path, device: Device, cache: &Path) -> anyhow::Result<Session> {
    validate(device)?;
    let builder = Session::builder()?.with_intra_threads(4)?;
    let builder = match device {
        Device::Cpu => builder,
        Device::Coreml => {
            #[cfg(feature = "coreml")]
            {
                use ort::execution_providers::{
                    CoreMLExecutionProvider,
                    coreml::{CoreMLComputeUnits, CoreMLModelFormat},
                };
                std::fs::create_dir_all(cache)?;
                builder.with_execution_providers([CoreMLExecutionProvider::default()
                    .with_model_format(CoreMLModelFormat::MLProgram)
                    .with_compute_units(CoreMLComputeUnits::All)
                    .with_static_input_shapes(true)
                    .with_model_cache_dir(cache.to_string_lossy())
                    .build()
                    .error_on_failure()])?
            }
            #[cfg(not(feature = "coreml"))]
            anyhow::bail!("CoreML feature is not compiled");
        }
        Device::Cuda => {
            #[cfg(feature = "ort-cuda")]
            {
                builder.with_execution_providers([
                    ort::execution_providers::CUDAExecutionProvider::default()
                        .build()
                        .error_on_failure(),
                ])?
            }
            #[cfg(not(feature = "ort-cuda"))]
            anyhow::bail!("CUDA feature is not compiled");
        }
        Device::Metal => anyhow::bail!("Metal is supported by the Candle Qwen adapter, not ORT"),
        Device::Auto => anyhow::bail!("auto requires calibration before constructing a session"),
    };
    let _ = cache;
    Ok(builder.commit_from_file(path)?)
}
