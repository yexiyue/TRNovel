use candle_core::{IndexOp, Module, Tensor};
use candle_nn::{GroupNorm, LayerNorm};

use crate::{
    audio_tokenizer::AudioTokenizerModelConfig,
    audio_tokenizer::parse_activation,
    codec_layers::{CodecConv1d, CodecLinear, CodecWeightSource},
    error::{OmniVoiceError, Result},
};

#[derive(Debug)]
pub(crate) struct HubertModel {
    feature_extractor: HubertFeatureExtractor,
    feature_projection: HubertFeatureProjection,
    encoder: HubertEncoder,
}

impl HubertModel {
    pub(crate) fn load(config: &AudioTokenizerModelConfig, ws: &CodecWeightSource) -> Result<Self> {
        Ok(Self {
            feature_extractor: HubertFeatureExtractor::load(config, &ws.pp("feature_extractor"))?,
            feature_projection: HubertFeatureProjection::load(
                config,
                &ws.pp("feature_projection"),
            )?,
            encoder: HubertEncoder::load(config, &ws.pp("encoder"))?,
        })
    }

    pub(crate) fn extract_semantic_features_from_resampled(
        &self,
        resampled: &Tensor,
        semantic_downsample_factor: usize,
    ) -> Result<Tensor> {
        let padded = resampled.pad_with_zeros(candle_core::D::Minus1, 160, 160)?;
        let extract_features = self.feature_extractor.forward(&padded)?;
        let extract_features = extract_features.transpose(1, 2)?;
        let hidden_states = self.feature_projection.forward(&extract_features)?;
        let mut semantic_features = self.encoder.forward_hidden_mean(&hidden_states)?;
        if semantic_downsample_factor > 1 {
            let seq_len = semantic_features.dim(1)?;
            let mut pieces = Vec::with_capacity(seq_len.div_ceil(semantic_downsample_factor));
            let mut index = 0;
            while index < seq_len {
                pieces.push(semantic_features.i((.., index..index + 1, ..))?);
                index += semantic_downsample_factor;
            }
            let piece_refs = pieces.iter().collect::<Vec<_>>();
            semantic_features = Tensor::cat(&piece_refs, 1)?;
        }
        Ok(semantic_features)
    }
}

#[derive(Debug)]
struct HubertFeatureExtractor {
    conv_layers: Vec<HubertConvLayer>,
}

impl HubertFeatureExtractor {
    fn load(config: &AudioTokenizerModelConfig, ws: &CodecWeightSource) -> Result<Self> {
        if config.semantic_feat_extract_norm != "group" {
            return Err(OmniVoiceError::Unsupported(format!(
                "unsupported Hubert feature extractor norm {}",
                config.semantic_feat_extract_norm
            )));
        }
        let mut conv_layers = Vec::with_capacity(config.semantic_conv_dim.len());
        for layer_index in 0..config.semantic_conv_dim.len() {
            let in_dim = if layer_index == 0 {
                1
            } else {
                config.semantic_conv_dim[layer_index - 1]
            };
            let out_dim = config.semantic_conv_dim[layer_index];
            let layer_ws = ws.pp("conv_layers").pp(layer_index);
            let conv = layer_ws.pp("conv").load_conv1d(
                in_dim,
                out_dim,
                config.semantic_conv_kernel[layer_index],
                config.semantic_conv_stride[layer_index],
                0,
                1,
                config.semantic_conv_bias,
                1,
            )?;
            let group_norm = if layer_index == 0 {
                Some(load_group_norm(&layer_ws.pp("layer_norm"), out_dim, 1e-5)?)
            } else {
                None
            };
            conv_layers.push(HubertConvLayer {
                conv,
                group_norm,
                activation: config.semantic_feat_extract_activation,
            });
        }
        Ok(Self { conv_layers })
    }

    fn forward(&self, input_values: &Tensor) -> Result<Tensor> {
        let mut hidden_states = input_values.unsqueeze(1)?;
        for layer in &self.conv_layers {
            hidden_states = layer.forward(&hidden_states)?;
        }
        Ok(hidden_states)
    }
}

#[derive(Debug)]
struct HubertConvLayer {
    conv: CodecConv1d,
    group_norm: Option<GroupNorm>,
    activation: candle_nn::Activation,
}

impl HubertConvLayer {
    fn forward(&self, hidden_states: &Tensor) -> Result<Tensor> {
        let mut hidden_states = self.conv.forward(hidden_states)?;
        if let Some(group_norm) = &self.group_norm {
            hidden_states = group_norm.forward(&hidden_states)?;
        }
        self.activation.forward(&hidden_states).map_err(Into::into)
    }
}

#[derive(Debug)]
struct HubertFeatureProjection {
    layer_norm: Option<LayerNorm>,
    projection: CodecLinear,
}

impl HubertFeatureProjection {
    fn load(config: &AudioTokenizerModelConfig, ws: &CodecWeightSource) -> Result<Self> {
        let layer_norm = if config.semantic_feat_proj_layer_norm {
            Some(load_layer_norm(
                &ws.pp("layer_norm"),
                config.semantic_conv_dim.last().copied().unwrap_or(512),
                config.semantic_layer_norm_eps,
            )?)
        } else {
            None
        };
        Ok(Self {
            layer_norm,
            projection: ws.pp("projection").load_linear(
                config.semantic_conv_dim.last().copied().unwrap_or(512),
                config.semantic_hidden_size,
                true,
            )?,
        })
    }

    fn forward(&self, hidden_states: &Tensor) -> Result<Tensor> {
        let hidden_states = if let Some(layer_norm) = &self.layer_norm {
            layer_norm.forward(hidden_states)?
        } else {
            hidden_states.clone()
        };
        self.projection.forward(&hidden_states).map_err(Into::into)
    }
}

#[derive(Debug)]
struct HubertEncoder {
    pos_conv_embed: HubertPositionalConvEmbedding,
    layer_norm: LayerNorm,
    layers: Vec<HubertEncoderLayer>,
}

impl HubertEncoder {
    fn load(config: &AudioTokenizerModelConfig, ws: &CodecWeightSource) -> Result<Self> {
        let mut layers = Vec::with_capacity(config.semantic_num_layers);
        for layer_index in 0..config.semantic_num_layers {
            layers.push(HubertEncoderLayer::load(
                config,
                &ws.pp("layers").pp(layer_index),
            )?);
        }
        Ok(Self {
            pos_conv_embed: HubertPositionalConvEmbedding::load(config, &ws.pp("pos_conv_embed"))?,
            layer_norm: load_layer_norm(
                &ws.pp("layer_norm"),
                config.semantic_hidden_size,
                config.semantic_layer_norm_eps,
            )?,
            layers,
        })
    }

    fn forward_hidden_mean(&self, hidden_states: &Tensor) -> Result<Tensor> {
        let position_embeddings = self.pos_conv_embed.forward(hidden_states)?;
        let mut hidden_states = hidden_states.broadcast_add(&position_embeddings)?;
        hidden_states = self.layer_norm.forward(&hidden_states)?;
        let mut hidden_sum = hidden_states.clone();
        let mut hidden_count = 1usize;
        for layer in &self.layers {
            hidden_states = layer.forward(&hidden_states)?;
            hidden_sum = hidden_sum.broadcast_add(&hidden_states)?;
            hidden_count += 1;
        }
        (hidden_sum / hidden_count as f64).map_err(Into::into)
    }
}

#[derive(Debug)]
struct HubertPositionalConvEmbedding {
    conv: CodecConv1d,
    activation: candle_nn::Activation,
    pad_remove: usize,
}

impl HubertPositionalConvEmbedding {
    fn load(config: &AudioTokenizerModelConfig, ws: &CodecWeightSource) -> Result<Self> {
        let conv = ws.pp("conv").load_weight_norm_conv1d(
            config.semantic_hidden_size,
            config.semantic_hidden_size / config.semantic_num_conv_pos_groups,
            config.semantic_num_conv_pos_embeddings,
            config.semantic_num_conv_pos_embeddings / 2,
            config.semantic_num_conv_pos_groups,
        )?;
        Ok(Self {
            conv,
            activation: parse_activation("gelu")?,
            pad_remove: usize::from(config.semantic_num_conv_pos_embeddings.is_multiple_of(2)),
        })
    }

    fn forward(&self, hidden_states: &Tensor) -> Result<Tensor> {
        let hidden_states = hidden_states.transpose(1, 2)?;
        let mut hidden_states = self.conv.forward(&hidden_states)?;
        if self.pad_remove > 0 {
            hidden_states = hidden_states.narrow(
                candle_core::D::Minus1,
                0,
                hidden_states.dim(candle_core::D::Minus1)? - self.pad_remove,
            )?;
        }
        let hidden_states = self.activation.forward(&hidden_states)?;
        hidden_states.transpose(1, 2).map_err(Into::into)
    }
}

#[derive(Debug)]
struct HubertEncoderLayer {
    attention: HubertAttention,
    layer_norm: LayerNorm,
    feed_forward: HubertFeedForward,
    final_layer_norm: LayerNorm,
}

impl HubertEncoderLayer {
    fn load(config: &AudioTokenizerModelConfig, ws: &CodecWeightSource) -> Result<Self> {
        Ok(Self {
            attention: HubertAttention::load(config, &ws.pp("attention"))?,
            layer_norm: load_layer_norm(
                &ws.pp("layer_norm"),
                config.semantic_hidden_size,
                config.semantic_layer_norm_eps,
            )?,
            feed_forward: HubertFeedForward::load(config, &ws.pp("feed_forward"))?,
            final_layer_norm: load_layer_norm(
                &ws.pp("final_layer_norm"),
                config.semantic_hidden_size,
                config.semantic_layer_norm_eps,
            )?,
        })
    }

    fn forward(&self, hidden_states: &Tensor) -> Result<Tensor> {
        let attn_residual = hidden_states.clone();
        let hidden_states = self.attention.forward(hidden_states)?;
        let hidden_states = hidden_states.broadcast_add(&attn_residual)?;
        let hidden_states = self.layer_norm.forward(&hidden_states)?;
        let hidden_states =
            hidden_states.broadcast_add(&self.feed_forward.forward(&hidden_states)?)?;
        self.final_layer_norm
            .forward(&hidden_states)
            .map_err(Into::into)
    }
}

#[derive(Debug)]
struct HubertAttention {
    q_proj: CodecLinear,
    k_proj: CodecLinear,
    v_proj: CodecLinear,
    out_proj: CodecLinear,
    num_heads: usize,
    head_dim: usize,
}

impl HubertAttention {
    fn load(config: &AudioTokenizerModelConfig, ws: &CodecWeightSource) -> Result<Self> {
        Ok(Self {
            q_proj: ws.pp("q_proj").load_linear(
                config.semantic_hidden_size,
                config.semantic_hidden_size,
                true,
            )?,
            k_proj: ws.pp("k_proj").load_linear(
                config.semantic_hidden_size,
                config.semantic_hidden_size,
                true,
            )?,
            v_proj: ws.pp("v_proj").load_linear(
                config.semantic_hidden_size,
                config.semantic_hidden_size,
                true,
            )?,
            out_proj: ws.pp("out_proj").load_linear(
                config.semantic_hidden_size,
                config.semantic_hidden_size,
                true,
            )?,
            num_heads: config.semantic_num_heads,
            head_dim: config.semantic_hidden_size / config.semantic_num_heads,
        })
    }

    fn forward(&self, hidden_states: &Tensor) -> Result<Tensor> {
        let (batch_size, seq_len, hidden_size) = hidden_states.dims3()?;
        let q = self
            .q_proj
            .forward(hidden_states)?
            .reshape((batch_size, seq_len, self.num_heads, self.head_dim))?
            .transpose(1, 2)?
            .contiguous()?;
        let k = self
            .k_proj
            .forward(hidden_states)?
            .reshape((batch_size, seq_len, self.num_heads, self.head_dim))?
            .transpose(1, 2)?
            .contiguous()?;
        let v = self
            .v_proj
            .forward(hidden_states)?
            .reshape((batch_size, seq_len, self.num_heads, self.head_dim))?
            .transpose(1, 2)?
            .contiguous()?;
        let attn_scores = (q.contiguous()?.matmul(&k.transpose(2, 3)?.contiguous()?)?
            / (self.head_dim as f64).sqrt())?;
        let attn_probs = candle_nn::ops::softmax_last_dim(&attn_scores)?;
        let context = attn_probs
            .contiguous()?
            .matmul(&v.contiguous()?)?
            .transpose(1, 2)?
            .reshape((batch_size, seq_len, hidden_size))?;
        self.out_proj.forward(&context).map_err(Into::into)
    }
}

#[derive(Debug)]
struct HubertFeedForward {
    intermediate_dense: CodecLinear,
    output_dense: CodecLinear,
    activation: candle_nn::Activation,
}

impl HubertFeedForward {
    fn load(config: &AudioTokenizerModelConfig, ws: &CodecWeightSource) -> Result<Self> {
        Ok(Self {
            intermediate_dense: ws.pp("intermediate_dense").load_linear(
                config.semantic_hidden_size,
                config.semantic_intermediate_size,
                true,
            )?,
            output_dense: ws.pp("output_dense").load_linear(
                config.semantic_intermediate_size,
                config.semantic_hidden_size,
                true,
            )?,
            activation: config.semantic_hidden_activation,
        })
    }

    fn forward(&self, hidden_states: &Tensor) -> Result<Tensor> {
        let hidden_states = self.intermediate_dense.forward(hidden_states)?;
        let hidden_states = self.activation.forward(&hidden_states)?;
        self.output_dense
            .forward(&hidden_states)
            .map_err(Into::into)
    }
}

#[derive(Debug)]
pub(crate) struct SemanticEncoder {
    conv: CodecConv1d,
    conv_blocks: Vec<SemanticEncoderBlock>,
}

impl SemanticEncoder {
    pub(crate) fn load(config: &AudioTokenizerModelConfig, ws: &CodecWeightSource) -> Result<Self> {
        let mut conv_blocks = Vec::with_capacity(config.strides.len());
        let mut in_channels = config.semantic_hidden_size;
        for (index, stride) in config.strides.iter().copied().enumerate() {
            let out_channels = config.semantic_hidden_size * config.channel_ratios[index];
            conv_blocks.push(SemanticEncoderBlock::load(
                config,
                in_channels,
                out_channels,
                stride,
                &ws.pp("conv_blocks").pp(index),
            )?);
            in_channels = out_channels;
        }
        Ok(Self {
            conv: ws.pp("conv").load_conv1d(
                config.semantic_hidden_size,
                config.semantic_hidden_size,
                config.kernel_size,
                1,
                config.kernel_size / 2,
                1,
                false,
                1,
            )?,
            conv_blocks,
        })
    }

    pub(crate) fn forward(&self, hidden_states: &Tensor) -> Result<Tensor> {
        let mut hidden_states = self.conv.forward(hidden_states)?;
        for block in &self.conv_blocks {
            hidden_states = block.forward(&hidden_states)?;
        }
        Ok(hidden_states)
    }
}

#[derive(Debug)]
struct SemanticEncoderBlock {
    residual_units: Vec<SemanticResidualUnit>,
    conv: CodecConv1d,
}

impl SemanticEncoderBlock {
    fn load(
        config: &AudioTokenizerModelConfig,
        in_channels: usize,
        out_channels: usize,
        stride: usize,
        ws: &CodecWeightSource,
    ) -> Result<Self> {
        let mut residual_units = Vec::with_capacity(config.block_dilations.len());
        for (index, dilation) in config.block_dilations.iter().copied().enumerate() {
            residual_units.push(SemanticResidualUnit::load(
                config,
                in_channels,
                out_channels.min(in_channels),
                dilation,
                &ws.pp("res_units").pp(index),
            )?);
        }
        let kernel = if stride == 1 { 3 } else { 2 * stride };
        let padding = (kernel - 1) / 2;
        Ok(Self {
            residual_units,
            conv: ws.pp("conv").load_conv1d(
                in_channels,
                out_channels,
                kernel,
                stride,
                padding,
                1,
                true,
                1,
            )?,
        })
    }

    fn forward(&self, hidden_states: &Tensor) -> Result<Tensor> {
        let mut hidden_states = hidden_states.clone();
        for unit in &self.residual_units {
            hidden_states = unit.forward(&hidden_states)?;
        }
        self.conv.forward(&hidden_states).map_err(Into::into)
    }
}

#[derive(Debug)]
struct SemanticResidualUnit {
    activation: candle_nn::Activation,
    conv1: CodecConv1d,
    conv2: CodecConv1d,
}

impl SemanticResidualUnit {
    fn load(
        config: &AudioTokenizerModelConfig,
        in_channels: usize,
        out_channels: usize,
        dilation: usize,
        ws: &CodecWeightSource,
    ) -> Result<Self> {
        let padding = ((config.unit_kernel_size - 1) / 2) * dilation;
        Ok(Self {
            activation: candle_nn::Activation::Elu(1.0),
            conv1: ws.pp("conv1").load_conv1d(
                in_channels,
                out_channels,
                config.unit_kernel_size,
                1,
                padding,
                1,
                false,
                1,
            )?,
            conv2: ws
                .pp("conv2")
                .load_conv1d(out_channels, out_channels, 1, 1, 0, 1, false, 1)?,
        })
    }

    fn forward(&self, hidden_states: &Tensor) -> Result<Tensor> {
        let residual = hidden_states.clone();
        let hidden_states = self.activation.forward(hidden_states)?;
        let hidden_states = self.conv1.forward(&hidden_states)?;
        let hidden_states = self.activation.forward(&hidden_states)?;
        let hidden_states = self.conv2.forward(&hidden_states)?;
        residual.broadcast_add(&hidden_states).map_err(Into::into)
    }
}

fn load_layer_norm(ws: &CodecWeightSource, hidden_size: usize, eps: f64) -> Result<LayerNorm> {
    Ok(LayerNorm::new(
        ws.get(hidden_size, "weight")?,
        ws.get(hidden_size, "bias")?,
        eps,
    ))
}

fn load_group_norm(ws: &CodecWeightSource, num_channels: usize, eps: f64) -> Result<GroupNorm> {
    GroupNorm::new(
        ws.get(num_channels, "weight")?,
        ws.get(num_channels, "bias")?,
        num_channels,
        num_channels,
        eps,
    )
    .map_err(Into::into)
}
