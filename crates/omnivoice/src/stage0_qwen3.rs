use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use candle_core::{DType, Module, Tensor};
use candle_nn::{Activation, Embedding, VarBuilder};
use candle_transformers::models::with_tracing::{Linear, RmsNorm, linear_b};

use crate::error::Result;

#[derive(Debug, Clone)]
pub struct Stage0Qwen3BackboneConfig {
    pub vocab_size: usize,
    pub hidden_size: usize,
    pub intermediate_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub num_key_value_heads: usize,
    pub head_dim: usize,
    pub max_position_embeddings: usize,
    pub attention_bias: bool,
    pub rms_norm_eps: f64,
    pub rope_theta: f64,
    pub hidden_act: Activation,
}

#[derive(Debug)]
pub struct Stage0BackboneOutput {
    pub final_hidden: Tensor,
    pub captured_hidden_layers: BTreeMap<usize, Tensor>,
}

pub type Stage0Qwen3Config = Stage0Qwen3BackboneConfig;
pub type Stage0ForwardPass = Stage0BackboneOutput;

#[derive(Debug, Clone)]
struct Stage0Qwen3RotaryEmbedding {
    sin: Tensor,
    cos: Tensor,
}

impl Stage0Qwen3RotaryEmbedding {
    fn new(dtype: DType, cfg: &Stage0Qwen3BackboneConfig, vb: &VarBuilder<'_>) -> Result<Self> {
        let dim = cfg.head_dim;
        let max_seq_len = cfg.max_position_embeddings;
        let inv_freq: Vec<_> = (0..dim)
            .step_by(2)
            .map(|index| 1f32 / cfg.rope_theta.powf(index as f64 / dim as f64) as f32)
            .collect();
        let inv_freq_len = inv_freq.len();
        let inv_freq =
            Tensor::from_vec(inv_freq, (1, inv_freq_len), vb.device())?.to_dtype(DType::F32)?;
        let t = Tensor::arange(0u32, max_seq_len as u32, vb.device())?
            .to_dtype(DType::F32)?
            .reshape((max_seq_len, 1))?;
        let freqs = t.matmul(&inv_freq)?;
        Ok(Self {
            sin: freqs.sin()?.to_dtype(dtype)?,
            cos: freqs.cos()?.to_dtype(dtype)?,
        })
    }

    fn apply(&self, q: &Tensor, k: &Tensor, offset: usize) -> Result<(Tensor, Tensor)> {
        let (_, _, seq_len, _) = q.dims4()?;
        let cos = self.cos.narrow(0, offset, seq_len)?;
        let sin = self.sin.narrow(0, offset, seq_len)?;
        let q_embed = candle_nn::rotary_emb::rope(&q.contiguous()?, &cos, &sin)?;
        let k_embed = candle_nn::rotary_emb::rope(&k.contiguous()?, &cos, &sin)?;
        Ok((q_embed, k_embed))
    }
}

#[derive(Debug, Clone)]
struct Stage0Qwen3Mlp {
    gate_proj: Linear,
    up_proj: Linear,
    down_proj: Linear,
    act_fn: Activation,
}

impl Stage0Qwen3Mlp {
    fn load(cfg: &Stage0Qwen3BackboneConfig, vb: VarBuilder<'_>) -> Result<Self> {
        Ok(Self {
            gate_proj: candle_transformers::models::with_tracing::linear_no_bias(
                cfg.hidden_size,
                cfg.intermediate_size,
                vb.pp("gate_proj"),
            )?,
            up_proj: candle_transformers::models::with_tracing::linear_no_bias(
                cfg.hidden_size,
                cfg.intermediate_size,
                vb.pp("up_proj"),
            )?,
            down_proj: candle_transformers::models::with_tracing::linear_no_bias(
                cfg.intermediate_size,
                cfg.hidden_size,
                vb.pp("down_proj"),
            )?,
            act_fn: cfg.hidden_act,
        })
    }
}

impl Module for Stage0Qwen3Mlp {
    fn forward(&self, xs: &Tensor) -> candle_core::Result<Tensor> {
        let lhs = xs.apply(&self.gate_proj)?.apply(&self.act_fn)?;
        let rhs = xs.apply(&self.up_proj)?;
        (lhs * rhs)?.apply(&self.down_proj)
    }
}

#[derive(Debug, Clone)]
struct Stage0Qwen3Attention {
    q_proj: Linear,
    k_proj: Linear,
    v_proj: Linear,
    o_proj: Linear,
    q_norm: RmsNorm,
    k_norm: RmsNorm,
    num_heads: usize,
    num_kv_heads: usize,
    head_dim: usize,
    hidden_size: usize,
    rotary_emb: Arc<Stage0Qwen3RotaryEmbedding>,
}

impl Stage0Qwen3Attention {
    fn load(
        cfg: &Stage0Qwen3BackboneConfig,
        rotary_emb: Arc<Stage0Qwen3RotaryEmbedding>,
        vb: VarBuilder<'_>,
    ) -> Result<Self> {
        let head_dim = cfg.head_dim;
        let num_heads = cfg.num_attention_heads;
        let num_kv_heads = cfg.num_key_value_heads;
        Ok(Self {
            q_proj: linear_b(
                cfg.hidden_size,
                num_heads * head_dim,
                cfg.attention_bias,
                vb.pp("q_proj"),
            )?,
            k_proj: linear_b(
                cfg.hidden_size,
                num_kv_heads * head_dim,
                cfg.attention_bias,
                vb.pp("k_proj"),
            )?,
            v_proj: linear_b(
                cfg.hidden_size,
                num_kv_heads * head_dim,
                cfg.attention_bias,
                vb.pp("v_proj"),
            )?,
            o_proj: linear_b(
                num_heads * head_dim,
                cfg.hidden_size,
                cfg.attention_bias,
                vb.pp("o_proj"),
            )?,
            q_norm: RmsNorm::new(head_dim, cfg.rms_norm_eps, vb.pp("q_norm"))?,
            k_norm: RmsNorm::new(head_dim, cfg.rms_norm_eps, vb.pp("k_norm"))?,
            num_heads,
            num_kv_heads,
            head_dim,
            hidden_size: num_heads * head_dim,
            rotary_emb,
        })
    }

    fn forward(
        &self,
        xs: &Tensor,
        attention_mask: Option<&Tensor>,
        flash_seqlens: Option<&[usize]>,
    ) -> Result<Tensor> {
        let (batch, seq_len, _) = xs.dims3()?;
        let q = self.q_proj.forward(xs)?;
        let k = self.k_proj.forward(xs)?;
        let v = self.v_proj.forward(xs)?;

        let q = q
            .reshape((batch, seq_len, self.num_heads, self.head_dim))?
            .transpose(1, 2)?;
        let k = k
            .reshape((batch, seq_len, self.num_kv_heads, self.head_dim))?
            .transpose(1, 2)?;
        let v = v
            .reshape((batch, seq_len, self.num_kv_heads, self.head_dim))?
            .transpose(1, 2)?;

        let q_flat = q.flatten(0, 2)?;
        let k_flat = k.flatten(0, 2)?;
        let q = apply_hf_compatible_rms_norm(&self.q_norm, &q_flat)?.reshape((
            batch,
            self.num_heads,
            seq_len,
            self.head_dim,
        ))?;
        let k = apply_hf_compatible_rms_norm(&self.k_norm, &k_flat)?.reshape((
            batch,
            self.num_kv_heads,
            seq_len,
            self.head_dim,
        ))?;

        let (q, k) = self.rotary_emb.apply(&q, &k, 0)?;
        let scale_f32 = 1.0f32 / (self.head_dim as f32).sqrt();
        let context = self.attention_context(q, k, v, attention_mask, flash_seqlens, scale_f32)?;
        Ok(context
            .transpose(1, 2)?
            .contiguous()?
            .reshape((batch, seq_len, self.hidden_size))?
            .apply(&self.o_proj)?)
    }

    fn attention_context(
        &self,
        q: Tensor,
        k: Tensor,
        v: Tensor,
        attention_mask: Option<&Tensor>,
        flash_seqlens: Option<&[usize]>,
        scale_f32: f32,
    ) -> Result<Tensor> {
        let _ = (scale_f32, flash_seqlens);
        let repeats = self.num_heads / self.num_kv_heads;
        let k = candle_transformers::utils::repeat_kv(k, repeats)?;
        let v = candle_transformers::utils::repeat_kv(v, repeats)?;
        let scale = 1.0 / (self.head_dim as f64).sqrt();
        let k_t = k.transpose(2, 3)?.contiguous()?;
        let mut scores = q.matmul(&k_t)?.affine(scale, 0.0)?;
        if let Some(mask) = attention_mask {
            scores = scores.broadcast_add(mask)?;
        }
        let probs = candle_nn::ops::softmax_last_dim(&scores)?;
        Ok(probs.matmul(&v)?)
    }
}

#[derive(Debug, Clone)]
struct Stage0Qwen3DecoderLayer {
    self_attn: Stage0Qwen3Attention,
    mlp: Stage0Qwen3Mlp,
    input_layernorm: RmsNorm,
    post_attention_layernorm: RmsNorm,
}

impl Stage0Qwen3DecoderLayer {
    fn load(
        cfg: &Stage0Qwen3BackboneConfig,
        rotary: Arc<Stage0Qwen3RotaryEmbedding>,
        vb: VarBuilder<'_>,
    ) -> Result<Self> {
        Ok(Self {
            self_attn: Stage0Qwen3Attention::load(cfg, rotary, vb.pp("self_attn"))?,
            mlp: Stage0Qwen3Mlp::load(cfg, vb.pp("mlp"))?,
            input_layernorm: RmsNorm::new(
                cfg.hidden_size,
                cfg.rms_norm_eps,
                vb.pp("input_layernorm"),
            )?,
            post_attention_layernorm: RmsNorm::new(
                cfg.hidden_size,
                cfg.rms_norm_eps,
                vb.pp("post_attention_layernorm"),
            )?,
        })
    }

    fn forward(
        &self,
        xs: &Tensor,
        attention_mask: Option<&Tensor>,
        flash_seqlens: Option<&[usize]>,
    ) -> Result<Tensor> {
        let residual = xs;
        let hidden = apply_hf_compatible_rms_norm(&self.input_layernorm, xs)?;
        let hidden = self
            .self_attn
            .forward(&hidden, attention_mask, flash_seqlens)?;
        let hidden = (hidden + residual)?;
        let residual = &hidden;
        let post_attention = apply_hf_compatible_rms_norm(&self.post_attention_layernorm, &hidden)?;
        let mlp_hidden = self.mlp.forward(&post_attention)?;
        Ok((residual + mlp_hidden)?)
    }
}

#[derive(Debug, Clone)]
pub struct Stage0Qwen3Backbone {
    embed_tokens: Embedding,
    layers: Vec<Stage0Qwen3DecoderLayer>,
    norm: RmsNorm,
}

impl Stage0Qwen3Backbone {
    pub fn load(cfg: &Stage0Qwen3BackboneConfig, vb: VarBuilder<'_>) -> Result<Self> {
        let embed_tokens =
            candle_nn::embedding(cfg.vocab_size, cfg.hidden_size, vb.pp("model.embed_tokens"))?;
        let rotary = Arc::new(Stage0Qwen3RotaryEmbedding::new(vb.dtype(), cfg, &vb)?);
        let layer_vb = vb.pp("model.layers");
        let mut layers = Vec::with_capacity(cfg.num_hidden_layers);
        for index in 0..cfg.num_hidden_layers {
            layers.push(Stage0Qwen3DecoderLayer::load(
                cfg,
                rotary.clone(),
                layer_vb.pp(index),
            )?);
        }
        let norm = RmsNorm::new(cfg.hidden_size, cfg.rms_norm_eps, vb.pp("model.norm"))?;
        Ok(Self {
            embed_tokens,
            layers,
            norm,
        })
    }

    pub fn text_embeddings(&self, input_ids: &Tensor) -> Result<Tensor> {
        Ok(self.embed_tokens.forward(input_ids)?)
    }

    pub fn embed_text_tokens(&self, input_ids: &Tensor) -> Result<Tensor> {
        self.text_embeddings(input_ids)
    }

    pub fn forward_embeds(
        &self,
        input_embeds: &Tensor,
        attention_mask: Option<&Tensor>,
        capture_layers: &BTreeSet<usize>,
    ) -> Result<Stage0BackboneOutput> {
        self.forward_embeds_with_flash(input_embeds, attention_mask, capture_layers, None)
    }

    pub fn forward_embeds_with_flash(
        &self,
        input_embeds: &Tensor,
        attention_mask: Option<&Tensor>,
        capture_layers: &BTreeSet<usize>,
        flash_seqlens: Option<&[usize]>,
    ) -> Result<Stage0BackboneOutput> {
        let mut hidden = input_embeds.clone();
        let mut captures = BTreeMap::new();
        for (index, layer) in self.layers.iter().enumerate() {
            hidden = layer.forward(&hidden, attention_mask, flash_seqlens)?;
            // HuggingFace exposes `hidden_states[layer_index + 1]` before the
            // final model norm.  Keep the last decoder layer in that same
            // form; callers that need the post-norm output already receive it
            // separately as `final_hidden`.
            if capture_layers.contains(&index) {
                captures.insert(index, hidden.clone());
            }
        }
        let final_hidden = apply_hf_compatible_rms_norm(&self.norm, &hidden)?;
        Ok(Stage0BackboneOutput {
            final_hidden,
            captured_hidden_layers: captures,
        })
    }
}

fn apply_hf_compatible_rms_norm(norm: &RmsNorm, xs: &Tensor) -> Result<Tensor> {
    if matches!(xs.dtype(), DType::F16 | DType::BF16) {
        Ok(norm.forward_diff(xs)?)
    } else {
        Ok(norm.forward(xs)?)
    }
}

#[allow(dead_code)]
fn try_flash_attention_per_row(
    _q: &Tensor,
    _k: &Tensor,
    _v: &Tensor,
    _seqlens: &[usize],
    _scale: f32,
) -> Result<Option<Tensor>> {
    Ok(None)
}
