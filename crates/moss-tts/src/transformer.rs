//! Qwen3 global and position-free local transformers.
use crate::config::TransformerConfig;
use candle_core::{DType, Module, Result, Tensor};
use candle_nn::{Linear, RmsNorm, VarBuilder, linear_no_bias, rms_norm};

pub(crate) struct Mlp {
    gate: Linear,
    up: Linear,
    down: Linear,
}
impl Mlp {
    pub(crate) fn load(
        input: usize,
        middle: usize,
        output: usize,
        vb: VarBuilder<'_>,
    ) -> Result<Self> {
        Ok(Self {
            gate: linear_no_bias(input, middle, vb.pp("gate_proj"))?,
            up: linear_no_bias(input, middle, vb.pp("up_proj"))?,
            down: linear_no_bias(middle, output, vb.pp("down_proj"))?,
        })
    }
    pub(crate) fn forward(&self, x: &Tensor) -> Result<Tensor> {
        self.down
            .forward(&(self.gate.forward(x)?.silu()? * self.up.forward(x)?)?)
    }
}

/// Queries use absolute positions; the codec additionally limits causal context.
pub(crate) fn attend(
    q: &Tensor,
    k: &Tensor,
    v: &Tensor,
    query_offset: usize,
    key_offset: usize,
    context: Option<usize>,
) -> Result<Tensor> {
    let (_, _, qlen, dim) = q.dims4()?;
    let klen = k.dim(2)?;
    let scores =
        (q.contiguous()?.matmul(&k.transpose(2, 3)?.contiguous()?)? / (dim as f64).sqrt())?;
    let mask: Vec<u8> = (0..qlen)
        .flat_map(|i| {
            (0..klen).map(move |j| {
                let qp = query_offset + i;
                let kp = key_offset + j;
                u8::from(kp > qp || context.is_some_and(|c| qp.saturating_sub(kp) >= c))
            })
        })
        .collect();
    let mask =
        Tensor::from_vec(mask, (1, 1, qlen, klen), q.device())?.broadcast_as(scores.shape())?;
    let negative = Tensor::full(f32::NEG_INFINITY, scores.shape(), scores.device())?
        .to_dtype(scores.dtype())?;
    let scores = mask.where_cond(&negative, &scores)?;
    candle_nn::ops::softmax_last_dim(&scores)?.matmul(&v.contiguous()?)
}

pub(crate) fn rope(x: &Tensor, offset: usize, theta: f64, interleaved: bool) -> Result<Tensor> {
    let (_, _, length, dim) = x.dims4()?;
    let frequencies: Vec<f32> = (0..dim / 2)
        .map(|i| theta.powf(-((2 * i) as f64) / dim as f64) as f32)
        .collect();
    let positions = Tensor::arange(offset as u32, (offset + length) as u32, x.device())?
        .to_dtype(DType::F32)?
        .unsqueeze(1)?;
    let angles = positions.matmul(&Tensor::from_vec(frequencies, (1, dim / 2), x.device())?)?;
    let cos = angles.cos()?.to_dtype(x.dtype())?;
    let sin = angles.sin()?.to_dtype(x.dtype())?;
    if interleaved {
        candle_nn::rotary_emb::rope_i(&x.contiguous()?, &cos, &sin)
    } else {
        candle_nn::rotary_emb::rope(&x.contiguous()?, &cos, &sin)
    }
}

struct Layer {
    q: Linear,
    k: Linear,
    v: Linear,
    out: Linear,
    qnorm: RmsNorm,
    knorm: RmsNorm,
    prenorm: RmsNorm,
    postnorm: RmsNorm,
    mlp: Mlp,
    cache: Option<(Tensor, Tensor)>,
    cfg: TransformerConfig,
}
impl Layer {
    fn load(cfg: &TransformerConfig, vb: VarBuilder<'_>) -> Result<Self> {
        let a = vb.pp("self_attn");
        let d = cfg.hidden_size;
        let h = cfg.head_dim;
        Ok(Self {
            q: linear_no_bias(d, cfg.num_attention_heads * h, a.pp("q_proj"))?,
            k: linear_no_bias(d, cfg.num_key_value_heads * h, a.pp("k_proj"))?,
            v: linear_no_bias(d, cfg.num_key_value_heads * h, a.pp("v_proj"))?,
            out: linear_no_bias(cfg.num_attention_heads * h, d, a.pp("o_proj"))?,
            qnorm: rms_norm(h, cfg.rms_norm_eps, a.pp("q_norm"))?,
            knorm: rms_norm(h, cfg.rms_norm_eps, a.pp("k_norm"))?,
            prenorm: rms_norm(d, cfg.rms_norm_eps, vb.pp("input_layernorm"))?,
            postnorm: rms_norm(d, cfg.rms_norm_eps, vb.pp("post_attention_layernorm"))?,
            mlp: Mlp::load(d, cfg.intermediate_size, d, vb.pp("mlp"))?,
            cache: None,
            cfg: cfg.clone(),
        })
    }
    fn forward(&mut self, x: &Tensor, offset: usize, rotary: bool) -> Result<Tensor> {
        let (b, t, _) = x.dims3()?;
        let c = &self.cfg;
        let norm = self.prenorm.forward(x)?;
        let q = self
            .qnorm
            .forward(
                &self
                    .q
                    .forward(&norm)?
                    .reshape((b, t, c.num_attention_heads, c.head_dim))?,
            )?
            .transpose(1, 2)?;
        let k = self
            .knorm
            .forward(
                &self
                    .k
                    .forward(&norm)?
                    .reshape((b, t, c.num_key_value_heads, c.head_dim))?,
            )?
            .transpose(1, 2)?;
        let v = self
            .v
            .forward(&norm)?
            .reshape((b, t, c.num_key_value_heads, c.head_dim))?
            .transpose(1, 2)?;
        let (q, k) = if rotary {
            (
                rope(&q, offset, c.rope_theta, false)?,
                rope(&k, offset, c.rope_theta, false)?,
            )
        } else {
            (q, k)
        };
        let (k, v) = match &self.cache {
            Some((pk, pv)) => (Tensor::cat(&[pk, &k], 2)?, Tensor::cat(&[pv, &v], 2)?),
            None => (k, v),
        };
        self.cache = Some((k.clone(), v.clone()));
        let k = candle_transformers::utils::repeat_kv(
            k,
            c.num_attention_heads / c.num_key_value_heads,
        )?;
        let v = candle_transformers::utils::repeat_kv(
            v,
            c.num_attention_heads / c.num_key_value_heads,
        )?;
        let attention = attend(&q, &k, &v, offset, 0, None)?
            .transpose(1, 2)?
            .reshape((b, t, c.num_attention_heads * c.head_dim))?;
        let x = (x + self.out.forward(&attention)?)?;
        &x + self.mlp.forward(&self.postnorm.forward(&x)?)?
    }
}

pub(crate) struct Transformer {
    layers: Vec<Layer>,
    norm: RmsNorm,
    rotary: bool,
    offset: usize,
}
impl Transformer {
    pub(crate) fn load(cfg: &TransformerConfig, vb: VarBuilder<'_>, rotary: bool) -> Result<Self> {
        Ok(Self {
            layers: (0..cfg.num_hidden_layers)
                .map(|i| Layer::load(cfg, vb.pp(format!("layers.{i}"))))
                .collect::<Result<_>>()?,
            norm: rms_norm(cfg.hidden_size, cfg.rms_norm_eps, vb.pp("norm"))?,
            rotary,
            offset: 0,
        })
    }
    pub(crate) fn reset(&mut self) {
        self.offset = 0;
        for layer in &mut self.layers {
            layer.cache = None;
        }
    }
    pub(crate) fn forward(
        &mut self,
        x: &Tensor,
        cancelled: &impl Fn() -> bool,
    ) -> anyhow::Result<Tensor> {
        let mut x = x.clone();
        for layer in &mut self.layers {
            crate::check_cancel(cancelled)?;
            x = layer.forward(&x, self.offset, self.rotary)?;
        }
        self.offset += x.dim(1)?;
        Ok(self.norm.forward(&x)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::Device;

    #[test]
    fn upstream_global_and_depth_match_full_and_incremental() -> anyhow::Result<()> {
        let fixture: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/upstream.json"))?;
        let config: TransformerConfig = serde_json::from_value(fixture["config"].clone())?;
        let dev = Device::Cpu;
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/upstream.safetensors");
        // SAFETY: checked-in fixtures are immutable throughout the test.
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[path], DType::F32, &dev)? };
        let input: Vec<f32> = serde_json::from_value(fixture["input"].clone())?;
        let input = Tensor::from_vec(input, (1, 5, 8), &dev)?;
        for (prefix, rotary, key) in [
            ("global", true, "global_output"),
            ("local", false, "local_output"),
        ] {
            let mut model = Transformer::load(&config, vb.pp(prefix), rotary)?;
            let expected: Vec<f32> = serde_json::from_value(fixture[key].clone())?;
            let full = model
                .forward(&input, &|| false)?
                .flatten_all()?
                .to_vec1::<f32>()?;
            let maximum = full
                .iter()
                .zip(&expected)
                .map(|(a, b)| (a - b).abs())
                .fold(0f32, f32::max);
            assert!(maximum < 1e-5, "{prefix} error: {maximum}");
            model.reset();
            let mut incremental = Vec::new();
            for i in 0..5 {
                incremental.extend(
                    model
                        .forward(&input.narrow(1, i, 1)?, &|| false)?
                        .flatten_all()?
                        .to_vec1::<f32>()?,
                );
            }
            let maximum = incremental
                .iter()
                .zip(&expected)
                .map(|(a, b)| (a - b).abs())
                .fold(0f32, f32::max);
            assert!(maximum < 1e-5, "{prefix} cache error: {maximum}");
            assert!(model.forward(&input, &|| true).is_err());
        }
        Ok(())
    }
    #[test]
    fn causal_window_uses_absolute_positions() -> Result<()> {
        let dev = Device::Cpu;
        let q = Tensor::zeros((1, 1, 1, 2), DType::F32, &dev)?;
        let k = Tensor::zeros((1, 1, 3, 2), DType::F32, &dev)?;
        let v = Tensor::from_vec(vec![2f32, 2., 4., 4., 100., 100.], (1, 1, 3, 2), &dev)?;
        assert_eq!(
            attend(&q, &k, &v, 1, 0, Some(1))?
                .flatten_all()?
                .to_vec1::<f32>()?,
            vec![4., 4.]
        );
        Ok(())
    }
}
