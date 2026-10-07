//! Native causal MOSS audio tokenizer with persistent decoder attention state.
use crate::transformer::{attend, rope};
use candle_core::{DType, Device, Module, Tensor};
use candle_nn::{Embedding, LayerNorm, Linear, VarBuilder, embedding, layer_norm, linear_no_bias};
use serde::Deserialize;
use std::path::Path;

#[derive(Deserialize)]
struct Config {
    sampling_rate: u32,
    downsample_rate: usize,
    causal_transformer_context_duration: f64,
    encoder_kwargs: Vec<StageConfig>,
    decoder_kwargs: Vec<StageConfig>,
    quantizer_kwargs: QuantizerConfig,
}
#[derive(Deserialize)]
struct QuantizerConfig {
    input_dim: usize,
    rvq_dim: usize,
    output_dim: usize,
    num_quantizers: usize,
    codebook_size: usize,
    codebook_dim: usize,
}
#[derive(Deserialize)]
struct StageConfig {
    module_type: String,
    #[serde(default)]
    patch_size: usize,
    #[serde(default)]
    input_dimension: usize,
    #[serde(default)]
    output_dimension: usize,
    #[serde(default)]
    d_model: usize,
    #[serde(default)]
    num_heads: usize,
    #[serde(default)]
    num_layers: usize,
    #[serde(default)]
    dim_feedforward: usize,
    #[serde(default)]
    max_period: f64,
}

struct Layer {
    input: Linear,
    output: Linear,
    norm1: LayerNorm,
    norm2: LayerNorm,
    ff1: Linear,
    ff2: Linear,
    scale1: Tensor,
    scale2: Tensor,
    heads: usize,
    theta: f64,
    context: usize,
    offset: usize,
    cache: Option<(Tensor, Tensor)>,
}
impl Layer {
    fn load(c: &StageConfig, context: usize, vb: VarBuilder<'_>) -> candle_core::Result<Self> {
        Ok(Self {
            input: linear_no_bias(c.d_model, 3 * c.d_model, vb.pp("self_attn.in_projs.0"))?,
            output: linear_no_bias(c.d_model, c.d_model, vb.pp("self_attn.out_projs.0"))?,
            norm1: layer_norm(c.d_model, 1e-5, vb.pp("norm1"))?,
            norm2: layer_norm(c.d_model, 1e-5, vb.pp("norm2"))?,
            ff1: linear_no_bias(c.d_model, c.dim_feedforward, vb.pp("linear1"))?,
            ff2: linear_no_bias(c.dim_feedforward, c.d_model, vb.pp("linear2"))?,
            scale1: vb.get(c.d_model, "layer_scale_1.scale")?,
            scale2: vb.get(c.d_model, "layer_scale_2.scale")?,
            heads: c.num_heads,
            theta: c.max_period,
            context,
            offset: 0,
            cache: None,
        })
    }
    fn reset(&mut self) {
        self.cache = None;
        self.offset = 0;
    }
    fn forward(&mut self, x: &Tensor, streaming: bool) -> candle_core::Result<Tensor> {
        let (b, t, d) = x.dims3()?;
        let head = d / self.heads;
        let qkv = self
            .input
            .forward(&self.norm1.forward(x)?)?
            .reshape((b, t, 3, self.heads, head))?
            .permute((2, 0, 3, 1, 4))?;
        let q = rope(&qkv.get(0)?, self.offset, self.theta, true)?;
        let k = rope(&qkv.get(1)?, self.offset, self.theta, true)?;
        let v = qkv.get(2)?;
        let previous = self.cache.as_ref().map_or(0, |(k, _)| k.dims()[2]);
        let (k, v) = match &self.cache {
            Some((pk, pv)) => (Tensor::cat(&[pk, &k], 2)?, Tensor::cat(&[pv, &v], 2)?),
            None => (k, v),
        };
        // Upstream overwrites old ring-cache keys before attending to a chunk.
        // Offline encoding keeps the full causal window.
        let length = k.dim(2)?;
        let keep = if streaming {
            length.min(self.context)
        } else {
            length
        };
        let removed = length - keep;
        let k = k.narrow(2, removed, keep)?.contiguous()?;
        let v = v.narrow(2, removed, keep)?.contiguous()?;
        let attended = attend(
            &q,
            &k,
            &v,
            self.offset,
            self.offset - previous + removed,
            Some(self.context),
        )?
        .transpose(1, 2)?
        .reshape((b, t, d))?;
        self.cache = Some((k, v));
        self.offset += t;
        let x = (x + self
            .output
            .forward(&attended)?
            .broadcast_mul(&self.scale1)?)?;
        let update = self
            .ff2
            .forward(&self.ff1.forward(&self.norm2.forward(&x)?)?.gelu_erf()?)?
            .broadcast_mul(&self.scale2)?;
        &x + update
    }
}

enum Stage {
    Patch(usize),
    Transformer {
        input: Option<Linear>,
        output: Option<Linear>,
        layers: Vec<Layer>,
    },
}
impl Stage {
    fn load(c: &StageConfig, context: usize, vb: VarBuilder<'_>) -> anyhow::Result<Self> {
        if c.module_type == "PatchedPretransform" {
            anyhow::ensure!(c.patch_size > 0, "invalid codec patch");
            return Ok(Self::Patch(c.patch_size));
        }
        anyhow::ensure!(
            c.module_type == "Transformer"
                && c.num_heads > 0
                && c.d_model.is_multiple_of(c.num_heads),
            "unsupported codec stage"
        );
        Ok(Self::Transformer {
            input: if c.input_dimension == c.d_model {
                None
            } else {
                Some(linear_no_bias(
                    c.input_dimension,
                    c.d_model,
                    vb.pp("input_proj"),
                )?)
            },
            output: if c.output_dimension == c.d_model {
                None
            } else {
                Some(linear_no_bias(
                    c.d_model,
                    c.output_dimension,
                    vb.pp("output_proj"),
                )?)
            },
            layers: (0..c.num_layers)
                .map(|i| Layer::load(c, context, vb.pp(format!("transformer.layers.{i}"))))
                .collect::<candle_core::Result<_>>()?,
        })
    }
    fn reset(&mut self) {
        if let Self::Transformer { layers, .. } = self {
            for layer in layers {
                layer.reset();
            }
        }
    }
    fn forward(
        &mut self,
        x: &Tensor,
        encode: bool,
        cancelled: &impl Fn() -> bool,
    ) -> anyhow::Result<Tensor> {
        crate::check_cancel(cancelled)?;
        Ok(match self {
            Self::Patch(size) => {
                let (b, d, t) = x.dims3()?;
                if encode {
                    x.reshape((b, d, t / *size, *size))?
                        .permute((0, 1, 3, 2))?
                        .reshape((b, d * *size, t / *size))?
                } else {
                    x.reshape((b, d / *size, *size, t))?
                        .permute((0, 1, 3, 2))?
                        .reshape((b, d / *size, t * *size))?
                }
            }
            Self::Transformer {
                input,
                output,
                layers,
            } => {
                let mut x = x.transpose(1, 2)?;
                if let Some(proj) = input {
                    x = proj.forward(&x)?;
                }
                for layer in layers {
                    crate::check_cancel(cancelled)?;
                    x = layer.forward(&x, !encode)?;
                }
                if let Some(proj) = output {
                    x = proj.forward(&x)?;
                }
                x.transpose(1, 2)?
            }
        })
    }
}

/// Weight-normalized 1x1 convolution used by residual codebooks, in F32.
struct Projection {
    weight: Tensor,
    bias: Tensor,
}
impl Projection {
    fn load(input: usize, output: usize, vb: VarBuilder<'_>) -> candle_core::Result<Self> {
        let v = vb
            .get((output, input, 1), "parametrizations.weight.original1")?
            .squeeze(2)?;
        let g = vb
            .get((output, 1, 1), "parametrizations.weight.original0")?
            .squeeze(2)?;
        let weight = v
            .broadcast_div(&v.sqr()?.sum_keepdim(1)?.sqrt()?)?
            .broadcast_mul(&g)?;
        Ok(Self {
            weight,
            bias: vb.get(output, "bias")?,
        })
    }
    fn forward(&self, x: &Tensor) -> candle_core::Result<Tensor> {
        Linear::new(self.weight.clone(), Some(self.bias.clone()))
            .forward(&x.transpose(1, 2)?)?
            .transpose(1, 2)
    }
}
struct Codebook {
    embedding: Embedding,
    input: Projection,
    output: Projection,
}
struct Quantizer {
    books: Vec<Codebook>,
    input: Option<Projection>,
    output: Option<Projection>,
    dimension: usize,
}
impl Quantizer {
    fn load(c: &QuantizerConfig, vb: VarBuilder<'_>) -> candle_core::Result<Self> {
        Ok(Self {
            books: (0..c.num_quantizers)
                .map(|i| {
                    let vb = vb.pp(format!("quantizers.{i}"));
                    Ok(Codebook {
                        embedding: embedding(c.codebook_size, c.codebook_dim, vb.pp("codebook"))?,
                        input: Projection::load(c.rvq_dim, c.codebook_dim, vb.pp("in_proj"))?,
                        output: Projection::load(c.codebook_dim, c.rvq_dim, vb.pp("out_proj"))?,
                    })
                })
                .collect::<candle_core::Result<_>>()?,
            input: if c.input_dim == c.rvq_dim {
                None
            } else {
                Some(Projection::load(
                    c.input_dim,
                    c.rvq_dim,
                    vb.pp("input_proj"),
                )?)
            },
            output: if c.output_dim == c.rvq_dim {
                None
            } else {
                Some(Projection::load(
                    c.rvq_dim,
                    c.output_dim,
                    vb.pp("output_proj"),
                )?)
            },
            dimension: c.rvq_dim,
        })
    }
    fn decode(&self, frames: &[Vec<u32>], device: &Device) -> anyhow::Result<Tensor> {
        let channels = frames[0].len();
        anyhow::ensure!(
            channels <= self.books.len()
                && frames
                    .iter()
                    .all(|f| f.len() == channels && f.iter().all(|id| *id < 1024)),
            "invalid codec tokens"
        );
        let mut x = Tensor::zeros((1, self.dimension, frames.len()), DType::F32, device)?;
        for (i, book) in self.books.iter().take(channels).enumerate() {
            let ids = Tensor::from_vec(
                frames.iter().map(|f| f[i]).collect::<Vec<_>>(),
                (1, frames.len()),
                device,
            )?;
            x = (&x
                + book
                    .output
                    .forward(&book.embedding.forward(&ids)?.transpose(1, 2)?)?)?;
        }
        if let Some(proj) = &self.output {
            x = proj.forward(&x)?;
        }
        Ok(x)
    }
    fn encode(
        &self,
        x: &Tensor,
        channels: usize,
        cancelled: &impl Fn() -> bool,
    ) -> anyhow::Result<Vec<Vec<u32>>> {
        anyhow::ensure!(
            channels > 0 && channels <= self.books.len(),
            "invalid codec codebook count"
        );
        let mut residual = x.to_dtype(DType::F32)?;
        if let Some(proj) = &self.input {
            residual = proj.forward(&residual)?;
        }
        let mut frames = vec![Vec::with_capacity(channels); x.dim(2)?];
        for book in self.books.iter().take(channels) {
            crate::check_cancel(cancelled)?;
            let latents = book.input.forward(&residual)?.transpose(1, 2)?.squeeze(0)?;
            let normalized = latents.broadcast_div(
                &latents
                    .sqr()?
                    .sum_keepdim(1)?
                    .sqrt()?
                    .clamp(1e-12, f64::MAX)?,
            )?;
            let codes = book.embedding.embeddings();
            let codes = codes.broadcast_div(
                &codes
                    .sqr()?
                    .sum_keepdim(1)?
                    .sqrt()?
                    .clamp(1e-12, f64::MAX)?,
            )?;
            let logits = normalized.matmul(&codes.t()?.contiguous()?)?;
            let ids = logits.argmax(1)?;
            for (frame, id) in frames.iter_mut().zip(ids.to_vec1::<u32>()?) {
                frame.push(id);
            }
            residual = (&residual
                - book.output.forward(
                    &book
                        .embedding
                        .forward(&ids.unsqueeze(0)?)?
                        .transpose(1, 2)?,
                )?)?;
        }
        Ok(frames)
    }
}

pub struct AudioCodec {
    encoder: Vec<Stage>,
    decoder: Vec<Stage>,
    quantizer: Quantizer,
    device: Device,
    dtype: DType,
    hop: usize,
    pub sample_rate: u32,
}
impl AudioCodec {
    pub fn load(directory: &Path, device: &Device, dtype: DType) -> anyhow::Result<Self> {
        let c: Config = serde_json::from_slice(&std::fs::read(directory.join("config.json"))?)?;
        anyhow::ensure!(
            c.sampling_rate == 24000 && c.downsample_rate == 1920,
            "unsupported codec configuration"
        );
        let vb = crate::weights(directory, dtype, device)?;
        let build = |configs: &[StageConfig],
                     name: &str,
                     mut rate: f64,
                     encode: bool|
         -> anyhow::Result<Vec<Stage>> {
            configs
                .iter()
                .enumerate()
                .map(|(i, stage)| {
                    let result = Stage::load(
                        stage,
                        (rate * c.causal_transformer_context_duration) as usize,
                        vb.pp(format!("{name}.{i}")),
                    )?;
                    if stage.module_type == "PatchedPretransform" {
                        if encode {
                            rate /= stage.patch_size as f64;
                        } else {
                            rate *= stage.patch_size as f64;
                        }
                    }
                    Ok(result)
                })
                .collect()
        };
        let encoder = build(&c.encoder_kwargs, "encoder", c.sampling_rate as f64, true)?;
        let decoder = build(
            &c.decoder_kwargs,
            "decoder",
            c.sampling_rate as f64 / c.downsample_rate as f64,
            false,
        )?;
        let quantizer = Quantizer::load(
            &c.quantizer_kwargs,
            crate::weights(directory, DType::F32, device)?.pp("quantizer"),
        )?;
        Ok(Self {
            encoder,
            decoder,
            quantizer,
            device: device.clone(),
            dtype,
            hop: c.downsample_rate,
            sample_rate: c.sampling_rate,
        })
    }
    pub fn reset_decoder(&mut self) {
        for stage in &mut self.decoder {
            stage.reset();
        }
    }
    /// Decode consecutive complete frames; call reset_decoder before a new utterance.
    pub fn decode(
        &mut self,
        frames: &[Vec<u32>],
        cancelled: &impl Fn() -> bool,
    ) -> anyhow::Result<Vec<f32>> {
        anyhow::ensure!(!frames.is_empty(), "empty codec frames");
        let mut x = self
            .quantizer
            .decode(frames, &self.device)?
            .to_dtype(self.dtype)?;
        for stage in &mut self.decoder {
            x = stage.forward(&x, false, cancelled)?;
        }
        let samples = x.flatten_all()?.to_dtype(DType::F32)?.to_vec1::<f32>()?;
        anyhow::ensure!(
            samples.len() == frames.len() * self.hop && samples.iter().all(|v| v.is_finite()),
            "invalid codec waveform"
        );
        Ok(samples)
    }
    /// Encode 24 kHz mono reference audio, zero-padding the final codec frame.
    pub fn encode(
        &mut self,
        samples: &[f32],
        channels: usize,
        cancelled: &impl Fn() -> bool,
    ) -> anyhow::Result<Vec<Vec<u32>>> {
        anyhow::ensure!(
            !samples.is_empty() && samples.iter().all(|v| v.is_finite()),
            "invalid reference waveform"
        );
        for stage in &mut self.encoder {
            stage.reset();
        }
        let mut padded = samples.to_vec();
        padded.resize(samples.len().div_ceil(self.hop) * self.hop, 0.);
        let mut x = Tensor::from_vec(padded.clone(), (1, 1, padded.len()), &self.device)?
            .to_dtype(self.dtype)?;
        for stage in &mut self.encoder {
            x = stage.forward(&x, true, cancelled)?;
        }
        self.quantizer.encode(&x, channels, cancelled)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn upstream_codec_matches_chunked_state() -> anyhow::Result<()> {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/upstream.json"))?;
        let config: StageConfig = serde_json::from_value(fixture["codec_config"].clone())?;
        let dev = Device::Cpu;
        let path =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/upstream.safetensors");
        // SAFETY: checked-in fixtures are immutable throughout the test.
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[path], DType::F32, &dev)? };
        let input: Vec<f32> = serde_json::from_value(fixture["codec_input"].clone())?;
        let input = Tensor::from_vec(input, (1, 4, 5), &dev)?;
        let expected: Vec<f32> = serde_json::from_value(fixture["codec_output"].clone())?;
        let mut stage = Stage::load(&config, 8, vb.pp("codec"))?;
        let full = stage
            .forward(&input, false, &|| false)?
            .flatten_all()?
            .to_vec1::<f32>()?;
        let maximum = full
            .iter()
            .zip(&expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(maximum < 1e-5, "codec error: {maximum}");
        stage.reset();
        let first = stage.forward(&input.narrow(2, 0, 2)?, false, &|| false)?;
        let second = stage.forward(&input.narrow(2, 2, 3)?, false, &|| false)?;
        let chunked = Tensor::cat(&[first, second], 2)?
            .flatten_all()?
            .to_vec1::<f32>()?;
        let maximum = chunked
            .iter()
            .zip(&expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(maximum < 1e-5, "codec streaming error: {maximum}");
        stage.reset();
        let input: Vec<f32> = serde_json::from_value(fixture["codec_wrap_input"].clone())?;
        let input = Tensor::from_vec(input, (1, 4, 21), &dev)?;
        let expected: Vec<f32> = serde_json::from_value(fixture["codec_wrap_output"].clone())?;
        let mut chunks = Vec::new();
        for start in (0..21).step_by(5) {
            chunks.push(stage.forward(
                &input.narrow(2, start, (21 - start).min(5))?,
                false,
                &|| false,
            )?);
        }
        let output = Tensor::cat(&chunks, 2)?.flatten_all()?.to_vec1::<f32>()?;
        let maximum = output
            .iter()
            .zip(&expected)
            .map(|(a, b)| (a - b).abs())
            .fold(0f32, f32::max);
        assert!(maximum < 1e-5, "codec ring boundary error: {maximum}");
        assert!(stage.forward(&input, false, &|| true).is_err());
        Ok(())
    }
}
