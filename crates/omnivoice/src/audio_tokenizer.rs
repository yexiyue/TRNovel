use std::{collections::BTreeSet, fs, path::Path, sync::OnceLock};

use candle_core::{DType, Device, IndexOp, Tensor, quantized::gguf_file};
use candle_nn::Module;
use serde::Deserialize;

use crate::{
    artifacts::{
        AudioTokenizerArtifacts, GgufBundleArtifacts, RuntimeArtifacts, gguf_array_usize, gguf_f64,
        gguf_u32,
    },
    codec_layers::{CodecLinear, CodecWeightSource},
    error::{OmniVoiceError, Result},
    runtime::RuntimeOptions,
};

#[path = "audio_tokenizer_dac.rs"]
mod audio_tokenizer_dac;
#[path = "audio_tokenizer_hubert.rs"]
mod audio_tokenizer_hubert;

use audio_tokenizer_dac::{AcousticEncoder, ResidualVectorQuantizer};
use audio_tokenizer_hubert::{HubertModel, SemanticEncoder};

#[derive(Debug, Clone, Deserialize)]
struct AudioTokenizerConfigFile {
    target_bandwidths: Vec<f32>,
    sample_rate: u32,
    semantic_sample_rate: u32,
    downsample_factor: usize,
    codebook_size: usize,
    codebook_dim: usize,
    kernel_size: usize,
    channel_ratios: Vec<usize>,
    strides: Vec<usize>,
    block_dilations: Vec<usize>,
    unit_kernel_size: usize,
    acoustic_model_config: AcousticModelConfigFile,
    semantic_model_config: SemanticModelConfigFile,
}

#[derive(Debug, Clone, Deserialize)]
struct AcousticModelConfigFile {
    encoder_hidden_size: usize,
    downsampling_ratios: Vec<usize>,
    hidden_size: usize,
}

#[derive(Debug, Clone, Deserialize)]
struct SemanticModelConfigFile {
    conv_bias: bool,
    conv_dim: Vec<usize>,
    conv_kernel: Vec<usize>,
    conv_stride: Vec<usize>,
    feat_extract_activation: String,
    feat_extract_norm: String,
    feat_proj_layer_norm: bool,
    hidden_act: String,
    hidden_size: usize,
    intermediate_size: usize,
    layer_norm_eps: f64,
    num_attention_heads: usize,
    num_conv_pos_embedding_groups: usize,
    num_conv_pos_embeddings: usize,
    num_hidden_layers: usize,
}

#[derive(Debug, Clone)]
pub(crate) struct AudioTokenizerModelConfig {
    pub sample_rate: u32,
    pub semantic_sample_rate: u32,
    pub downsample_factor: usize,
    pub target_bandwidths: Vec<f32>,
    pub codebook_size: usize,
    pub codebook_dim: usize,
    pub kernel_size: usize,
    pub channel_ratios: Vec<usize>,
    pub strides: Vec<usize>,
    pub block_dilations: Vec<usize>,
    pub unit_kernel_size: usize,
    pub acoustic_hidden_size: usize,
    pub acoustic_encoder_hidden_size: usize,
    pub acoustic_downsampling_ratios: Vec<usize>,
    pub semantic_hidden_size: usize,
    pub semantic_intermediate_size: usize,
    pub semantic_num_heads: usize,
    pub semantic_num_layers: usize,
    pub semantic_layer_norm_eps: f64,
    pub semantic_conv_bias: bool,
    pub semantic_conv_dim: Vec<usize>,
    pub semantic_conv_kernel: Vec<usize>,
    pub semantic_conv_stride: Vec<usize>,
    pub semantic_feat_extract_norm: String,
    pub semantic_feat_extract_activation: candle_nn::Activation,
    pub semantic_feat_proj_layer_norm: bool,
    pub semantic_hidden_activation: candle_nn::Activation,
    pub semantic_num_conv_pos_embeddings: usize,
    pub semantic_num_conv_pos_groups: usize,
}

impl AudioTokenizerModelConfig {
    pub fn from_artifacts(audio_tokenizer: &AudioTokenizerArtifacts) -> Result<Self> {
        let raw: AudioTokenizerConfigFile =
            serde_json::from_str(&fs::read_to_string(audio_tokenizer.config_path())?)?;
        let config = Self {
            sample_rate: raw.sample_rate,
            semantic_sample_rate: raw.semantic_sample_rate,
            downsample_factor: raw.downsample_factor,
            target_bandwidths: raw.target_bandwidths,
            codebook_size: raw.codebook_size,
            codebook_dim: raw.codebook_dim,
            kernel_size: raw.kernel_size,
            channel_ratios: raw.channel_ratios,
            strides: raw.strides,
            block_dilations: raw.block_dilations,
            unit_kernel_size: raw.unit_kernel_size,
            acoustic_hidden_size: raw.acoustic_model_config.hidden_size,
            acoustic_encoder_hidden_size: raw.acoustic_model_config.encoder_hidden_size,
            acoustic_downsampling_ratios: raw.acoustic_model_config.downsampling_ratios,
            semantic_hidden_size: raw.semantic_model_config.hidden_size,
            semantic_intermediate_size: raw.semantic_model_config.intermediate_size,
            semantic_num_heads: raw.semantic_model_config.num_attention_heads,
            semantic_num_layers: raw.semantic_model_config.num_hidden_layers,
            semantic_layer_norm_eps: raw.semantic_model_config.layer_norm_eps,
            semantic_conv_bias: raw.semantic_model_config.conv_bias,
            semantic_conv_dim: raw.semantic_model_config.conv_dim,
            semantic_conv_kernel: raw.semantic_model_config.conv_kernel,
            semantic_conv_stride: raw.semantic_model_config.conv_stride,
            semantic_feat_extract_norm: raw.semantic_model_config.feat_extract_norm,
            semantic_feat_extract_activation: parse_activation(
                &raw.semantic_model_config.feat_extract_activation,
            )?,
            semantic_feat_proj_layer_norm: raw.semantic_model_config.feat_proj_layer_norm,
            semantic_hidden_activation: parse_activation(&raw.semantic_model_config.hidden_act)?,
            semantic_num_conv_pos_embeddings: raw.semantic_model_config.num_conv_pos_embeddings,
            semantic_num_conv_pos_groups: raw.semantic_model_config.num_conv_pos_embedding_groups,
        };
        config.validate()
    }

    pub fn from_gguf_bundle(gguf: &GgufBundleArtifacts) -> Result<Self> {
        let content = gguf.open_audio_tokenizer_content()?;
        let sample_rate = gguf_u32(&content, "omnivoice.sample_rate")?;
        let semantic_sample_rate = gguf_u32(&content, "omnivoice.semantic_sample_rate")?;
        let downsample_factor = gguf_u32(&content, "omnivoice.downsample_factor")? as usize;
        let codebook_size = gguf_u32(&content, "omnivoice.codebook_size")? as usize;
        let codebook_dim = gguf_u32(&content, "omnivoice.codebook_dim")? as usize;
        let acoustic_encoder_hidden_size =
            gguf_u32(&content, "omnivoice.acoustic.encoder_hidden_size")? as usize;
        let acoustic_hidden_size = gguf_u32(&content, "omnivoice.acoustic.hidden_size")? as usize;
        let acoustic_downsampling_ratios =
            gguf_array_usize(&content, "omnivoice.acoustic.downsampling_ratios")?;
        let layout = gguf_semantic_encoder_layout(&content)?;
        let hop_length = acoustic_downsampling_ratios
            .iter()
            .try_fold(1usize, |product, ratio| product.checked_mul(*ratio))
            .unwrap_or(0);
        let num_quantizers = gguf_quantizer_count(&content)?;
        let target_bandwidths = target_bandwidths_for_quantizers(
            sample_rate,
            hop_length,
            codebook_size,
            num_quantizers,
        )?;
        let config = Self {
            sample_rate,
            semantic_sample_rate,
            downsample_factor,
            target_bandwidths,
            codebook_size,
            codebook_dim,
            kernel_size: layout.kernel_size,
            channel_ratios: layout.channel_ratios,
            strides: layout.strides,
            block_dilations: layout.block_dilations,
            unit_kernel_size: layout.unit_kernel_size,
            acoustic_hidden_size,
            acoustic_encoder_hidden_size,
            acoustic_downsampling_ratios,
            semantic_hidden_size: gguf_u32(&content, "omnivoice.semantic.hidden_size")? as usize,
            semantic_intermediate_size: gguf_u32(&content, "omnivoice.semantic.intermediate_size")?
                as usize,
            semantic_num_heads: gguf_u32(&content, "omnivoice.semantic.num_attention_heads")?
                as usize,
            semantic_num_layers: gguf_u32(&content, "omnivoice.semantic.num_hidden_layers")?
                as usize,
            semantic_layer_norm_eps: gguf_f64(&content, "omnivoice.semantic.layer_norm_eps")?,
            semantic_conv_bias: content
                .tensor_infos
                .contains_key("semantic_model.feature_extractor.conv_layers.0.conv.bias"),
            semantic_conv_dim: gguf_array_usize(&content, "omnivoice.semantic.conv_dim")?,
            semantic_conv_kernel: gguf_array_usize(&content, "omnivoice.semantic.conv_kernel")?,
            semantic_conv_stride: gguf_array_usize(&content, "omnivoice.semantic.conv_stride")?,
            semantic_feat_extract_norm: "group".to_string(),
            semantic_feat_extract_activation: parse_activation("gelu")?,
            semantic_feat_proj_layer_norm: content
                .tensor_infos
                .contains_key("semantic_model.feature_projection.layer_norm.weight"),
            semantic_hidden_activation: parse_activation("gelu")?,
            semantic_num_conv_pos_embeddings: gguf_u32(
                &content,
                "omnivoice.semantic.num_conv_pos_embeddings",
            )? as usize,
            semantic_num_conv_pos_groups: gguf_u32(
                &content,
                "omnivoice.semantic.num_conv_pos_embedding_groups",
            )? as usize,
        };
        let config = config.validate()?;
        if config.num_quantizers() != num_quantizers {
            return Err(OmniVoiceError::InvalidData(format!(
                "GGUF tokenizer quantizer count {num_quantizers} does not match derived num_quantizers {}",
                config.num_quantizers()
            )));
        }
        Ok(config)
    }

    fn validate(self) -> Result<Self> {
        if self.sample_rate == 0 || self.semantic_sample_rate == 0 {
            return Err(OmniVoiceError::InvalidData(
                "audio tokenizer sample rates must be greater than zero".to_string(),
            ));
        }
        if self.downsample_factor == 0 || self.hop_length() == 0 {
            return Err(OmniVoiceError::InvalidData(
                "audio tokenizer downsample_factor and hop length must be greater than zero"
                    .to_string(),
            ));
        }
        if self.codebook_size == 0 || self.codebook_dim == 0 {
            return Err(OmniVoiceError::InvalidData(
                "audio tokenizer codebook dimensions must be greater than zero".to_string(),
            ));
        }
        if self.codebook_size < 2 {
            return Err(OmniVoiceError::InvalidData(
                "audio tokenizer codebook_size must be at least two".to_string(),
            ));
        }
        if self.target_bandwidths.is_empty()
            || self
                .target_bandwidths
                .iter()
                .any(|value| !value.is_finite() || *value <= 0.0)
        {
            return Err(OmniVoiceError::InvalidData(
                "audio tokenizer target_bandwidths must contain finite positive values".to_string(),
            ));
        }
        if self.acoustic_downsampling_ratios.is_empty()
            || self.acoustic_downsampling_ratios.contains(&0)
            || self.hop_length() == 0
        {
            return Err(OmniVoiceError::InvalidData(
                "audio tokenizer acoustic downsampling ratios must be non-zero and fit usize"
                    .to_string(),
            ));
        }
        if self.strides.is_empty()
            || self.strides.len() != self.channel_ratios.len()
            || self.strides.contains(&0)
            || self.channel_ratios.contains(&0)
            || self.block_dilations.is_empty()
            || self.block_dilations.contains(&0)
            || self.unit_kernel_size == 0
            || self.kernel_size == 0
        {
            return Err(OmniVoiceError::InvalidData(
                "audio tokenizer encoder strides, channel ratios, kernels, and dilations are invalid"
                    .to_string(),
            ));
        }
        if self.semantic_conv_dim.len() != self.semantic_conv_kernel.len()
            || self.semantic_conv_dim.len() != self.semantic_conv_stride.len()
            || self.semantic_conv_dim.is_empty()
            || self.semantic_conv_dim.contains(&0)
            || self.semantic_conv_kernel.contains(&0)
            || self.semantic_conv_stride.contains(&0)
            || self.semantic_hidden_size == 0
            || self.semantic_intermediate_size == 0
            || self.acoustic_hidden_size == 0
            || self.acoustic_encoder_hidden_size == 0
            || self.semantic_num_heads == 0
            || !self
                .semantic_hidden_size
                .is_multiple_of(self.semantic_num_heads)
            || self.semantic_num_conv_pos_embeddings == 0
            || self.semantic_num_conv_pos_groups == 0
            || !self
                .semantic_hidden_size
                .is_multiple_of(self.semantic_num_conv_pos_groups)
        {
            return Err(OmniVoiceError::InvalidData(
                "audio tokenizer semantic model dimensions are inconsistent".to_string(),
            ));
        }
        Ok(self)
    }

    pub fn hop_length(&self) -> usize {
        self.acoustic_downsampling_ratios
            .iter()
            .try_fold(1usize, |product, ratio| product.checked_mul(*ratio))
            .unwrap_or(0)
    }

    pub fn num_quantizers(&self) -> usize {
        let bandwidth = self.target_bandwidths.last().copied().unwrap_or(2.0);
        let frame_rate = (self.sample_rate as f32 / self.hop_length() as f32).ceil();
        let codebook_bits = (self.codebook_size as f32).log2();
        ((1000.0 * bandwidth) / (frame_rate * codebook_bits))
            .floor()
            .max(1.0) as usize
    }

    pub fn semantic_downsample_factor(&self) -> usize {
        ((self.hop_length() as f32 / (self.sample_rate as f32 / self.semantic_sample_rate as f32))
            / self.downsample_factor as f32)
            .round()
            .max(1.0) as usize
    }
}

#[derive(Debug)]
pub struct AudioTokenizerRuntimePlan {
    device: Device,
    runtime_dtype: DType,
    config: AudioTokenizerModelConfig,
    weights_path: std::path::PathBuf,
    model: OnceLock<std::result::Result<AudioTokenizerModel, String>>,
}

impl AudioTokenizerRuntimePlan {
    pub fn from_runtime_artifacts(
        options: RuntimeOptions,
        runtime: &RuntimeArtifacts,
    ) -> Result<Self> {
        let device = options.resolve_device()?;
        Self::from_runtime_artifacts_with_device(options, runtime, device)
    }

    pub fn from_runtime_artifacts_with_device(
        options: RuntimeOptions,
        runtime: &RuntimeArtifacts,
        device: Device,
    ) -> Result<Self> {
        let runtime_dtype = options.resolve_audio_dtype_for_runtime_device(&device);
        let (config, weights_path) = if let Some(gguf) = runtime.gguf_bundle() {
            (
                AudioTokenizerModelConfig::from_gguf_bundle(gguf)?,
                gguf.audio_tokenizer_path().to_path_buf(),
            )
        } else {
            let audio_tokenizer = runtime.audio_tokenizer()?;
            (
                AudioTokenizerModelConfig::from_artifacts(audio_tokenizer)?,
                audio_tokenizer.weights_path().to_path_buf(),
            )
        };
        Ok(Self {
            device,
            runtime_dtype,
            config,
            weights_path,
            model: OnceLock::new(),
        })
    }

    pub fn encode_waveform(
        &self,
        samples: &[f32],
        sample_rate: u32,
    ) -> Result<crate::contracts::I64Tensor2> {
        let codes = self.encode_waveform_device(samples, sample_rate)?;
        let (quantizers, steps) = codes.dims2()?;
        let data = codes
            .to_device(&Device::Cpu)?
            .to_dtype(DType::I64)?
            .flatten_all()?
            .to_vec1::<i64>()?;
        crate::contracts::I64Tensor2::new((quantizers, steps), data)
    }

    pub fn encode_waveform_device(&self, samples: &[f32], sample_rate: u32) -> Result<Tensor> {
        if samples.is_empty() {
            return Err(OmniVoiceError::InvalidRequest(
                "audio tokenizer input waveform is empty".to_string(),
            ));
        }
        if let Some(index) = samples.iter().position(|sample| !sample.is_finite()) {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "audio tokenizer input sample at index {index} is not finite"
            )));
        }
        if sample_rate != self.config.sample_rate {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "audio tokenizer expects {} Hz input, got {sample_rate}",
                self.config.sample_rate
            )));
        }
        let waveform = Tensor::from_vec(samples.to_vec(), (1, 1, samples.len()), &self.device)?;
        let semantic_waveform = if self.config.sample_rate != self.config.semantic_sample_rate {
            let semantic_samples = crate::audio_input::resample_linear(
                samples,
                self.config.sample_rate,
                self.config.semantic_sample_rate,
            );
            let semantic_len = semantic_samples.len();
            Some(Tensor::from_vec(
                semantic_samples,
                (1, semantic_len),
                &self.device,
            )?)
        } else {
            None
        };
        let codes = self
            .model()?
            .encode(&waveform, semantic_waveform.as_ref())?;
        let codes = codes.i(0)?.to_dtype(DType::I64)?;
        let (quantizers, steps) = codes.dims2()?;
        if quantizers != self.config.num_quantizers() || steps == 0 {
            return Err(OmniVoiceError::InvalidTensorShape {
                name: "audio_tokenizer.codes".to_string(),
                expected: format!("({}, T>0)", self.config.num_quantizers()),
                actual: format!("({quantizers}, {steps})"),
            });
        }
        // Range check on-device (min/max) — avoid full D2H of codes on every
        // voice-clone encode, which used to serialize the CUDA stream.
        let codes_f = codes.to_dtype(DType::F32)?;
        let min_v = codes_f.min_all()?.to_scalar::<f32>()?;
        let max_v = codes_f.max_all()?.to_scalar::<f32>()?;
        if min_v < 0.0 || max_v >= self.config.codebook_size as f32 {
            return Err(OmniVoiceError::InvalidData(format!(
                "audio tokenizer produced a code outside [0, {})",
                self.config.codebook_size
            )));
        }
        Ok(codes)
    }

    fn model(&self) -> Result<&AudioTokenizerModel> {
        let result = self.model.get_or_init(|| {
            AudioTokenizerModel::load(
                &self.config,
                &self.weights_path,
                &self.device,
                self.runtime_dtype,
            )
            .map_err(|error| error.to_string())
        });
        match result {
            Ok(model) => Ok(model),
            Err(message) => Err(OmniVoiceError::InvalidData(message.clone())),
        }
    }
}

#[derive(Debug)]
struct AudioTokenizerModel {
    semantic_model: HubertModel,
    encoder_semantic: SemanticEncoder,
    acoustic_encoder: AcousticEncoder,
    fc: CodecLinear,
    quantizer: ResidualVectorQuantizer,
    config: AudioTokenizerModelConfig,
}

impl AudioTokenizerModel {
    fn load(
        config: &AudioTokenizerModelConfig,
        weights_path: &Path,
        device: &Device,
        dtype: DType,
    ) -> Result<Self> {
        let ws = CodecWeightSource::from_weight_path(weights_path, dtype, device)?;
        Ok(Self {
            semantic_model: HubertModel::load(config, &ws.pp("semantic_model"))?,
            encoder_semantic: SemanticEncoder::load(config, &ws.pp("encoder_semantic"))?,
            acoustic_encoder: AcousticEncoder::load(config, &ws.pp("acoustic_encoder"))?,
            fc: ws.pp("fc").load_linear(
                config.acoustic_hidden_size + config.semantic_hidden_size,
                config.acoustic_hidden_size + config.semantic_hidden_size,
                true,
            )?,
            quantizer: ResidualVectorQuantizer::load(config, &ws.pp("quantizer"))?,
            config: config.clone(),
        })
    }

    fn encode(&self, waveform: &Tensor, semantic_waveform: Option<&Tensor>) -> Result<Tensor> {
        let (_, channels, _) = waveform.dims3()?;
        if channels != 1 {
            return Err(OmniVoiceError::InvalidTensorShape {
                name: "audio_tokenizer.input_values".to_string(),
                expected: "(B, 1, T)".to_string(),
                actual: format!("{:?}", waveform.dims()),
            });
        }
        let semantic_input = match semantic_waveform {
            Some(semantic_waveform) => semantic_waveform.clone(),
            None => waveform.i((.., 0, ..))?,
        };
        let semantic_features = self
            .semantic_model
            .extract_semantic_features_from_resampled(
                &semantic_input,
                self.config.semantic_downsample_factor(),
            )?;
        let semantic_latents = self
            .encoder_semantic
            .forward(&semantic_features.transpose(1, 2)?)?;

        let acoustic_latents = {
            let raw = self.acoustic_encoder.forward(waveform)?;
            if raw.dim(candle_core::D::Minus1)? == semantic_latents.dim(candle_core::D::Minus1)? {
                raw
            } else {
                self.acoustic_encoder.forward(&waveform.pad_with_zeros(
                    candle_core::D::Minus1,
                    self.config.hop_length() / 2,
                    self.config.hop_length() / 2,
                )?)?
            }
        };

        let embeddings = Tensor::cat(&[&acoustic_latents, &semantic_latents], 1)?;
        let embeddings = self
            .fc
            .forward(&embeddings.transpose(1, 2)?)?
            .transpose(1, 2)?;
        self.quantizer.encode(&embeddings)
    }
}

pub(crate) fn parse_activation(name: &str) -> Result<candle_nn::Activation> {
    match name {
        "gelu" => Ok(candle_nn::Activation::Gelu),
        "silu" => Ok(candle_nn::Activation::Silu),
        "elu" => Ok(candle_nn::Activation::Elu(1.0)),
        other => Err(OmniVoiceError::Unsupported(format!(
            "unsupported activation {other}"
        ))),
    }
}

fn gguf_quantizer_count(content: &gguf_file::Content) -> Result<usize> {
    let mut indices = BTreeSet::new();
    for name in content.tensor_infos.keys() {
        let Some(rest) = name.strip_prefix("quantizer.quantizers.") else {
            continue;
        };
        let Some((index, _)) = rest.split_once('.') else {
            continue;
        };
        if let Ok(index) = index.parse::<usize>() {
            indices.insert(index);
        }
    }
    if indices.is_empty() {
        return Err(OmniVoiceError::InvalidData(
            "GGUF tokenizer is missing quantizer.quantizers tensors".to_string(),
        ));
    }
    let expected = (0..indices.len()).collect::<BTreeSet<_>>();
    if indices != expected {
        return Err(OmniVoiceError::InvalidData(format!(
            "GGUF tokenizer quantizer indices {indices:?} are not contiguous"
        )));
    }
    Ok(indices.len())
}

struct GgufSemanticEncoderLayout {
    kernel_size: usize,
    channel_ratios: Vec<usize>,
    strides: Vec<usize>,
    block_dilations: Vec<usize>,
    unit_kernel_size: usize,
}

fn gguf_semantic_encoder_layout(content: &gguf_file::Content) -> Result<GgufSemanticEncoderLayout> {
    let kernel_size = gguf_tensor_last_dim(content, "encoder_semantic.conv.weight")?;
    let unit_kernel_size = gguf_tensor_last_dim(
        content,
        "encoder_semantic.conv_blocks.0.res_units.0.conv1.weight",
    )?;
    let mut blocks = BTreeSet::new();
    let mut units = BTreeSet::new();
    for name in content.tensor_infos.keys() {
        let Some(rest) = name.strip_prefix("encoder_semantic.conv_blocks.") else {
            continue;
        };
        let mut parts = rest.split('.');
        let Some(block) = parts.next().and_then(|value| value.parse::<usize>().ok()) else {
            continue;
        };
        blocks.insert(block);
        if parts.next() == Some("res_units")
            && let Some(unit) = parts.next().and_then(|value| value.parse::<usize>().ok())
        {
            units.insert(unit);
        }
    }
    if blocks.is_empty() || units.is_empty() {
        return Err(OmniVoiceError::InvalidData(
            "GGUF tokenizer is missing encoder_semantic conv blocks".to_string(),
        ));
    }
    let n_blocks = blocks.iter().copied().max().unwrap_or(0) + 1;
    let n_units = units.iter().copied().max().unwrap_or(0) + 1;
    if blocks.len() != n_blocks || units.len() != n_units {
        return Err(OmniVoiceError::InvalidData(format!(
            "GGUF tokenizer encoder_semantic block/unit indices are not contiguous: blocks={blocks:?} units={units:?}"
        )));
    }
    Ok(GgufSemanticEncoderLayout {
        kernel_size,
        channel_ratios: vec![1; n_blocks],
        strides: vec![1; n_blocks],
        block_dilations: vec![1; n_units],
        unit_kernel_size,
    })
}

fn gguf_tensor_last_dim(content: &gguf_file::Content, name: &str) -> Result<usize> {
    let dims = content
        .tensor_infos
        .get(name)
        .map(|info| info.shape.dims())
        .ok_or_else(|| OmniVoiceError::InvalidData(format!("missing GGUF tensor {name}")))?;
    dims.last().copied().filter(|dim| *dim > 0).ok_or_else(|| {
        OmniVoiceError::InvalidData(format!("GGUF tensor {name} has an empty shape {dims:?}"))
    })
}

fn target_bandwidths_for_quantizers(
    sample_rate: u32,
    hop_length: usize,
    codebook_size: usize,
    num_quantizers: usize,
) -> Result<Vec<f32>> {
    if hop_length == 0 || codebook_size < 2 || num_quantizers == 0 {
        return Err(OmniVoiceError::InvalidData(
            "cannot derive GGUF tokenizer target_bandwidths".to_string(),
        ));
    }
    let frame_rate = (sample_rate as f32 / hop_length as f32).ceil();
    let codebook_bits = (codebook_size as f32).log2();
    let bandwidth = (num_quantizers as f32 * frame_rate * codebook_bits) / 1000.0;
    if !bandwidth.is_finite() || bandwidth <= 0.0 {
        return Err(OmniVoiceError::InvalidData(format!(
            "derived GGUF tokenizer target bandwidth {bandwidth} is invalid"
        )));
    }
    Ok(vec![bandwidth])
}
