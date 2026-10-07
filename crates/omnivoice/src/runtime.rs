use std::path::{Path, PathBuf};

use candle_core::{DType, Device};

use crate::{
    artifacts::{ModelWeightDType, RuntimeArtifactFormat, RuntimeArtifacts},
    error::{OmniVoiceError, Result},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DeviceSpec {
    #[default]
    Auto,
    Cpu,
    Cuda(usize),
    Metal,
}

impl DeviceSpec {
    pub fn parse(value: &str) -> Result<Self> {
        let normalized = value.trim().to_ascii_lowercase();
        if normalized == "auto" {
            return Ok(Self::Auto);
        }
        if normalized == "cpu" {
            return Ok(Self::Cpu);
        }
        if normalized == "cuda" {
            return Ok(Self::Cuda(0));
        }
        if normalized == "mps" {
            return Ok(Self::Metal);
        }
        if normalized == "metal" {
            return Ok(Self::Metal);
        }
        if let Some(index) = normalized.strip_prefix("cuda:") {
            let ordinal = index.parse::<usize>().map_err(|_| {
                OmniVoiceError::InvalidRequest(format!("invalid cuda device ordinal in {value}"))
            })?;
            return Ok(Self::Cuda(ordinal));
        }
        Err(OmniVoiceError::InvalidRequest(format!(
            "unsupported device spec {value}"
        )))
    }

    pub fn resolve(self) -> Result<Device> {
        match self {
            Self::Auto => resolve_auto_device(),
            Self::Cpu => Ok(Device::Cpu),
            Self::Cuda(index) => resolve_cuda_device(index),
            Self::Metal => resolve_metal_device(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DTypeSpec {
    #[default]
    Auto,
    F32,
    F16,
    BF16,
}

impl DTypeSpec {
    pub fn parse(value: &str) -> Result<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "f32" => Ok(Self::F32),
            "f16" => Ok(Self::F16),
            "bf16" => Ok(Self::BF16),
            other => Err(OmniVoiceError::InvalidRequest(format!(
                "unsupported dtype spec {other}"
            ))),
        }
    }

    pub fn resolve_for_device(self, device: DeviceSpec) -> DType {
        match self {
            Self::Auto => match device {
                // Stage0 backbone: f16 on GPU backends. Stage1/audio stay f32
                // via the audio dtype resolver.
                DeviceSpec::Cuda(_) | DeviceSpec::Metal | DeviceSpec::Auto => DType::F16,
                DeviceSpec::Cpu => DType::F32,
            },
            Self::F32 => DType::F32,
            Self::F16 => DType::F16,
            Self::BF16 => DType::BF16,
        }
    }

    pub fn resolve_for_runtime_device(self, device: &Device) -> DType {
        match self {
            Self::Auto => {
                if device_prefers_f16_activations(device) {
                    DType::F16
                } else {
                    DType::F32
                }
            }
            Self::F32 => DType::F32,
            Self::F16 => DType::F16,
            Self::BF16 => DType::BF16,
        }
    }
}

fn device_prefers_f16_activations(device: &Device) -> bool {
    device.is_cuda() || device.is_metal()
}

pub(crate) fn activation_dtype_for_weights(
    format: RuntimeArtifactFormat,
    weights: ModelWeightDType,
    device: &Device,
) -> DType {
    activation_dtype(format, weights, device_prefers_f16_activations(device))
}

fn activation_dtype(
    format: RuntimeArtifactFormat,
    weights: ModelWeightDType,
    on_gpu: bool,
) -> DType {
    match format {
        RuntimeArtifactFormat::SafetensorsManifest if !on_gpu => DType::F32,
        RuntimeArtifactFormat::GgufBundle if !on_gpu => DType::F32,
        RuntimeArtifactFormat::SafetensorsManifest => match weights {
            ModelWeightDType::BF16 => DType::BF16,
            ModelWeightDType::F16 | ModelWeightDType::F32 | ModelWeightDType::Quantized => {
                DType::F16
            }
        },
        // Candle QMatMul QTensor kernels accept f32/f16 activations only, not bf16.
        // On GPU use f16; CPU f16 is a scalar (no rayon) path and is much slower.
        RuntimeArtifactFormat::GgufBundle => DType::F16,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeOptions {
    model_root: PathBuf,
    device: DeviceSpec,
    dtype: DTypeSpec,
    seed: Option<u64>,
}

impl RuntimeOptions {
    pub fn new(model_root: impl Into<PathBuf>) -> Self {
        Self {
            model_root: model_root.into(),
            device: DeviceSpec::default(),
            dtype: DTypeSpec::default(),
            seed: None,
        }
    }

    pub fn with_device(mut self, device: DeviceSpec) -> Self {
        self.device = device;
        self
    }

    pub fn with_dtype(mut self, dtype: DTypeSpec) -> Self {
        self.dtype = dtype;
        self
    }

    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    pub fn model_root(&self) -> &Path {
        &self.model_root
    }

    pub fn device(&self) -> DeviceSpec {
        self.device
    }

    pub fn dtype(&self) -> DTypeSpec {
        self.dtype
    }

    pub fn seed(&self) -> Option<u64> {
        self.seed
    }

    pub fn resolve_device(&self) -> Result<Device> {
        self.device.resolve()
    }

    pub fn resolve_dtype(&self) -> DType {
        self.dtype.resolve_for_device(self.device)
    }

    pub fn resolve_dtype_for_runtime_device(&self, device: &Device) -> DType {
        self.dtype.resolve_for_runtime_device(device)
    }

    pub fn resolve_stage0_dtype(
        &self,
        device: &Device,
        runtime: &RuntimeArtifacts,
    ) -> Result<DType> {
        match self.dtype {
            DTypeSpec::Auto => {
                warn_if_cpu_quantized_without_simd(device, runtime);
                Ok(activation_dtype_for_weights(
                    runtime.format(),
                    runtime.native_weight_dtype()?,
                    device,
                ))
            }
            DTypeSpec::F32 | DTypeSpec::F16 | DTypeSpec::BF16 => {
                Ok(self.dtype.resolve_for_runtime_device(device))
            }
        }
    }

    pub fn resolve_audio_dtype_for_runtime_device(&self, _device: &Device) -> DType {
        // Audio tokenizer and stage1 decoder are numerically unstable in f16 on GPU backends.
        // Keep them on f32 until lower-precision parity is verified for live inference.
        DType::F32
    }

    pub fn load_runtime_artifacts(&self) -> Result<RuntimeArtifacts> {
        RuntimeArtifacts::from_model_root(&self.model_root)
    }
}

pub fn auto_device_resolution_order() -> Vec<DeviceSpec> {
    let mut order = Vec::with_capacity(5);
    #[cfg(feature = "cuda")]
    order.push(DeviceSpec::Cuda(0));
    #[cfg(all(feature = "metal", target_os = "macos"))]
    order.push(DeviceSpec::Metal);
    order.push(DeviceSpec::Cpu);
    order
}

fn resolve_auto_device() -> Result<Device> {
    for candidate in auto_device_resolution_order() {
        match candidate {
            DeviceSpec::Cuda(index) => {
                #[cfg(feature = "cuda")]
                if let Ok(device) = Device::new_cuda(index) {
                    return Ok(device);
                }
                #[cfg(not(feature = "cuda"))]
                let _ = index;
            }
            DeviceSpec::Metal =>
            {
                #[cfg(all(feature = "metal", target_os = "macos"))]
                if let Ok(device) = Device::new_metal(0) {
                    return Ok(device);
                }
            }
            DeviceSpec::Cpu => return Ok(Device::Cpu),
            DeviceSpec::Auto => {}
        }
    }
    Ok(Device::Cpu)
}

#[cfg(feature = "cuda")]
fn resolve_cuda_device(index: usize) -> Result<Device> {
    Ok(Device::new_cuda(index)?)
}

#[cfg(not(feature = "cuda"))]
fn resolve_cuda_device(index: usize) -> Result<Device> {
    Err(OmniVoiceError::Unsupported(format!(
        "cuda device cuda:{index} requires the `cuda` feature"
    )))
}

fn warn_if_cpu_quantized_without_simd(device: &Device, runtime: &RuntimeArtifacts) {
    let _ = (device, runtime);
    #[cfg(all(target_arch = "x86_64", not(target_feature = "avx2")))]
    if device.is_cpu()
        && matches!(
            runtime.native_weight_dtype(),
            Ok(ModelWeightDType::Quantized)
        )
    {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            eprintln!(
                "warning: GGUF Q4 CPU kernels compiled without AVX2; \
                 rebuild with -C target-cpu=native (see .cargo/config.toml)"
            );
        });
    }
}

#[cfg(all(feature = "metal", target_os = "macos"))]
fn resolve_metal_device() -> Result<Device> {
    Ok(Device::new_metal(0)?)
}

#[cfg(not(all(feature = "metal", target_os = "macos")))]
fn resolve_metal_device() -> Result<Device> {
    Err(OmniVoiceError::Unsupported(
        "metal device requires the `metal` feature".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{DeviceSpec, activation_dtype};
    use crate::artifacts::{ModelWeightDType, RuntimeArtifactFormat};
    use candle_core::DType;

    #[test]
    fn device_spec_accepts_official_aliases() {
        assert_eq!(DeviceSpec::parse("cuda").unwrap(), DeviceSpec::Cuda(0));
        assert_eq!(DeviceSpec::parse("mps").unwrap(), DeviceSpec::Metal);
    }

    #[test]
    fn device_spec_still_accepts_existing_spellings() {
        assert_eq!(DeviceSpec::parse("auto").unwrap(), DeviceSpec::Auto);
        assert_eq!(DeviceSpec::parse("cpu").unwrap(), DeviceSpec::Cpu);
        assert_eq!(DeviceSpec::parse("cuda:3").unwrap(), DeviceSpec::Cuda(3));
        assert_eq!(DeviceSpec::parse("metal").unwrap(), DeviceSpec::Metal);
    }

    #[test]
    fn auto_activations_are_f32_on_cpu() {
        assert_eq!(
            activation_dtype(
                RuntimeArtifactFormat::SafetensorsManifest,
                ModelWeightDType::F16,
                false,
            ),
            DType::F32
        );
        assert_eq!(
            activation_dtype(
                RuntimeArtifactFormat::SafetensorsManifest,
                ModelWeightDType::BF16,
                false,
            ),
            DType::F32
        );
        assert_eq!(
            activation_dtype(
                RuntimeArtifactFormat::SafetensorsManifest,
                ModelWeightDType::F32,
                false,
            ),
            DType::F32
        );
        assert_eq!(
            activation_dtype(
                RuntimeArtifactFormat::GgufBundle,
                ModelWeightDType::Quantized,
                false,
            ),
            DType::F32
        );
    }

    #[test]
    fn auto_activations_use_f16_or_bf16_elsewhere() {
        assert_eq!(
            activation_dtype(
                RuntimeArtifactFormat::GgufBundle,
                ModelWeightDType::Quantized,
                false,
            ),
            DType::F32
        );
        assert_eq!(
            activation_dtype(
                RuntimeArtifactFormat::GgufBundle,
                ModelWeightDType::BF16,
                true,
            ),
            DType::F16
        );
        assert_eq!(
            activation_dtype(
                RuntimeArtifactFormat::SafetensorsManifest,
                ModelWeightDType::BF16,
                true,
            ),
            DType::BF16
        );
        assert_eq!(
            activation_dtype(
                RuntimeArtifactFormat::SafetensorsManifest,
                ModelWeightDType::F16,
                true,
            ),
            DType::F16
        );
        assert_eq!(
            activation_dtype(
                RuntimeArtifactFormat::SafetensorsManifest,
                ModelWeightDType::F32,
                true,
            ),
            DType::F16
        );
    }
}
