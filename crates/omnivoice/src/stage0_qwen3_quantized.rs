use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use candle_core::{
    DType, Module, Tensor,
    quantized::{GgmlDType, QMatMul, QTensor},
};
use candle_nn::Activation;
use candle_transformers::quantized_var_builder::VarBuilder as QuantizedVarBuilder;

use crate::{
    codec_layers::QuantizedEmbedding,
    error::Result,
    stage0_qwen3::{Stage0BackboneOutput, Stage0Qwen3BackboneConfig},
};

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

#[derive(Debug, Clone)]
struct RotaryEmbedding {
    sin: Tensor,
    cos: Tensor,
}

impl RotaryEmbedding {
    fn new(cfg: &Stage0Qwen3BackboneConfig, device: &candle_core::Device) -> Result<Self> {
        let dim = cfg.head_dim;
        let max_seq_len = cfg.max_position_embeddings;
        let inv_freq: Vec<_> = (0..dim)
            .step_by(2)
            .map(|index| 1f32 / cfg.rope_theta.powf(index as f64 / dim as f64) as f32)
            .collect();
        let inv_freq_len = inv_freq.len();
        let inv_freq =
            Tensor::from_vec(inv_freq, (1, inv_freq_len), device)?.to_dtype(DType::F32)?;
        let t = Tensor::arange(0u32, max_seq_len as u32, device)?
            .to_dtype(DType::F32)?
            .reshape((max_seq_len, 1))?;
        let freqs = t.matmul(&inv_freq)?;
        Ok(Self {
            sin: freqs.sin()?,
            cos: freqs.cos()?,
        })
    }

    fn apply(&self, q: &Tensor, k: &Tensor, offset: usize) -> Result<(Tensor, Tensor)> {
        let (_, _, seq_len, _) = q.dims4()?;
        let cos = self.cos.narrow(0, offset, seq_len)?.to_dtype(q.dtype())?;
        let sin = self.sin.narrow(0, offset, seq_len)?.to_dtype(q.dtype())?;
        let q_embed = candle_nn::rotary_emb::rope(&q.contiguous()?, &cos, &sin)?;
        let k_embed = candle_nn::rotary_emb::rope(&k.contiguous()?, &cos, &sin)?;
        Ok((q_embed, k_embed))
    }
}

#[derive(Debug, Clone)]
struct QwenMlp {
    gate_proj: QMatMul,
    up_proj: QMatMul,
    down_proj: QMatMul,
    act_fn: Activation,
}

impl QwenMlp {
    fn load(cfg: &Stage0Qwen3BackboneConfig, vb: QuantizedVarBuilder) -> Result<Self> {
        Ok(Self {
            gate_proj: keep_quantized_matmul(get_quantized_qtensor_by_candidates(
                &vb,
                &["ffn_gate.weight", "ffn_gate", "mlp.gate_proj.weight"],
            )?)?,
            up_proj: keep_quantized_matmul(get_quantized_qtensor_by_candidates(
                &vb,
                &["ffn_up.weight", "ffn_up", "mlp.up_proj.weight"],
            )?)?,
            down_proj: keep_quantized_matmul(get_quantized_qtensor_by_candidates(
                &vb,
                &["ffn_down.weight", "ffn_down", "mlp.down_proj.weight"],
            )?)?,
            act_fn: cfg.hidden_act,
        })
    }
}

impl Module for QwenMlp {
    fn forward(&self, xs: &Tensor) -> candle_core::Result<Tensor> {
        let lhs = xs.apply(&self.gate_proj)?.apply(&self.act_fn)?;
        let rhs = xs.apply(&self.up_proj)?;
        (lhs * rhs)?.apply(&self.down_proj)
    }
}

#[derive(Debug, Clone)]
struct QwenAttention {
    q_proj: QMatMul,
    k_proj: QMatMul,
    v_proj: QMatMul,
    o_proj: QMatMul,
    q_norm: LocalRmsNorm,
    k_norm: LocalRmsNorm,
    num_heads: usize,
    num_kv_heads: usize,
    head_dim: usize,
    hidden_size: usize,
    rotary_emb: Arc<RotaryEmbedding>,
}

impl QwenAttention {
    fn load(
        cfg: &Stage0Qwen3BackboneConfig,
        rotary_emb: Arc<RotaryEmbedding>,
        vb: QuantizedVarBuilder,
        activation_dtype: DType,
    ) -> Result<Self> {
        let head_dim = cfg.head_dim;
        let num_heads = cfg.num_attention_heads;
        let num_kv_heads = cfg.num_key_value_heads;
        Ok(Self {
            q_proj: keep_quantized_matmul(get_quantized_qtensor_by_candidates(
                &vb,
                &[
                    "attn_q.weight",
                    "attn_q",
                    "self_attn.q_proj.weight",
                    "self_attn.q_proj",
                ],
            )?)?,
            k_proj: keep_quantized_matmul(get_quantized_qtensor_by_candidates(
                &vb,
                &[
                    "attn_k.weight",
                    "attn_k",
                    "self_attn.k_proj.weight",
                    "self_attn.k_proj",
                ],
            )?)?,
            v_proj: keep_quantized_matmul(get_quantized_qtensor_by_candidates(
                &vb,
                &[
                    "attn_v.weight",
                    "attn_v",
                    "self_attn.v_proj.weight",
                    "self_attn.v_proj",
                ],
            )?)?,
            o_proj: keep_quantized_matmul(get_quantized_qtensor_by_candidates(
                &vb,
                &[
                    "attn_output.weight",
                    "attn_output",
                    "self_attn.o_proj.weight",
                    "self_attn.o_proj",
                ],
            )?)?,
            q_norm: LocalRmsNorm::from_candidates(
                &vb,
                &[
                    "attn_q_norm.weight",
                    "attn_q_norm",
                    "self_attn.q_norm.weight",
                    "self_attn.q_norm",
                ],
                cfg.rms_norm_eps,
                activation_dtype,
            )?,
            k_norm: LocalRmsNorm::from_candidates(
                &vb,
                &[
                    "attn_k_norm.weight",
                    "attn_k_norm",
                    "self_attn.k_norm.weight",
                    "self_attn.k_norm",
                ],
                cfg.rms_norm_eps,
                activation_dtype,
            )?,
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
        // On builds without `feature="cuda"` the flash branch is compiled out,
        // so keep the parameter referenced to avoid `unused_variables`.
        let _ = flash_seqlens;
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
        let q = self.q_norm.forward(&q.flatten(0, 2)?)?.reshape((
            batch,
            self.num_heads,
            seq_len,
            self.head_dim,
        ))?;
        let k = self.k_norm.forward(&k.flatten(0, 2)?)?.reshape((
            batch,
            self.num_kv_heads,
            seq_len,
            self.head_dim,
        ))?;
        let (q, k) = self.rotary_emb.apply(&q, &k, 0)?;

        let repeats = self.num_heads / self.num_kv_heads;
        let k = candle_transformers::utils::repeat_kv(k, repeats)?;
        let v = candle_transformers::utils::repeat_kv(v, repeats)?;
        let scale = 1.0 / (self.head_dim as f64).sqrt();
        let k_t = k.transpose(2, 3)?.contiguous()?;
        let mut scores = q.matmul(&k_t)?.affine(scale, 0.0)?;
        if let Some(mask) = attention_mask {
            // Quantized matmuls hard-code F32 outputs on vulkan/wgpu regardless of
            // the runtime activation dtype, so `scores` can be F32 while the mask
            // prepared in stage0_model is the GGUF runtime dtype (F16 on GPU).
            // Normalize the mask to the scores dtype so the add is single-dtype on
            // every backend (no-op on cuda/cpu, where the dtypes already match).
            scores = scores.broadcast_add(&mask.to_dtype(scores.dtype())?)?;
        }
        let probs = candle_nn::ops::softmax_last_dim(&scores)?;
        let context = probs.matmul(&v)?;
        Ok(context
            .transpose(1, 2)?
            .contiguous()?
            .reshape((batch, seq_len, self.hidden_size))?
            .apply(&self.o_proj)?)
    }
}

#[derive(Debug, Clone)]
struct QwenDecoderLayer {
    self_attn: QwenAttention,
    mlp: QwenMlp,
    input_layernorm: LocalRmsNorm,
    post_attention_layernorm: LocalRmsNorm,
}

impl QwenDecoderLayer {
    fn load(
        cfg: &Stage0Qwen3BackboneConfig,
        rotary: Arc<RotaryEmbedding>,
        vb: QuantizedVarBuilder,
        activation_dtype: DType,
    ) -> Result<Self> {
        Ok(Self {
            self_attn: QwenAttention::load(cfg, rotary, vb.clone(), activation_dtype)?,
            mlp: QwenMlp::load(cfg, vb.clone())?,
            input_layernorm: LocalRmsNorm::from_candidates(
                &vb,
                &[
                    "attn_norm.weight",
                    "attn_norm",
                    "input_layernorm.weight",
                    "input_layernorm",
                ],
                cfg.rms_norm_eps,
                activation_dtype,
            )?,
            post_attention_layernorm: LocalRmsNorm::from_candidates(
                &vb,
                &[
                    "ffn_norm.weight",
                    "ffn_norm",
                    "post_attention_layernorm.weight",
                    "post_attention_layernorm",
                ],
                cfg.rms_norm_eps,
                activation_dtype,
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
        let hidden = self.input_layernorm.forward(xs)?;
        let hidden = self
            .self_attn
            .forward(&hidden, attention_mask, flash_seqlens)?;
        // Attention output can be F32 (vulkan/wgpu quantized matmuls) while the
        // running residual is the GGUF runtime dtype (F16 on GPU) — cast back so
        // the add is single-dtype on every backend (no-op on cuda/cpu).
        let hidden = (hidden.to_dtype(residual.dtype())? + residual)?;
        let residual = &hidden;
        let post_attention = self.post_attention_layernorm.forward(&hidden)?;
        let mlp_hidden = self.mlp.forward(&post_attention)?;
        // Same normalization: MLP output carries the quantized-matmul output dtype.
        Ok((residual + mlp_hidden.to_dtype(residual.dtype())?)?)
    }
}

#[derive(Debug, Clone)]
pub struct Stage0Qwen3QuantizedBackbone {
    embed_tokens: QuantizedEmbedding,
    layers: Vec<QwenDecoderLayer>,
    norm: LocalRmsNorm,
}

#[derive(Debug, Clone)]
struct LocalRmsNorm {
    weight: Tensor,
    eps: f64,
}

impl LocalRmsNorm {
    fn from_candidates(
        vb: &QuantizedVarBuilder,
        names: &[&str],
        eps: f64,
        activation_dtype: DType,
    ) -> Result<Self> {
        let qtensor = get_qtensor_by_candidates(vb, names)?;
        // 1D norms are not quantized matmuls; dequant the tiny vector once.
        let weight = qtensor
            .dequantize(vb.device())?
            .to_dtype(activation_dtype)?;
        Ok(Self { weight, eps })
    }

    fn forward(&self, xs: &Tensor) -> Result<Tensor> {
        let weight = self.weight.to_dtype(xs.dtype())?;
        candle_nn::ops::rms_norm(xs, &weight, self.eps as f32).map_err(Into::into)
    }
}

pub(crate) fn keep_quantized_matmul(weight: Arc<QTensor>) -> Result<QMatMul> {
    // CUDA quantized kernels accept *packed* Q4/Q8, not float weights.
    // If GGUF tensor alias is actually float (F32/F16/BF16), fall back to dense
    // matmul and dequantize directly to f16 to keep memory down.
    //
    // NOTE: use `dequantize(...)` + `to_dtype(F16)` rather than
    // `dequantize_f16(...)`. On CUDA, `QTensor::dequantize_f16` dispatches to a
    // specialized kernel path that only supports packed-quant source dtypes and
    // bails with "unsupported dtype for dequantize F32" for float sources
    // (candle quantized/cuda.rs dequantize_f16 has no float-source branch).
    // Plain `dequantize` handles F32/F16/BF16 sources on every backend:
    // cuda via the CPU-fallback path (quantized/cuda.rs:679-681), wgpu via the
    // raw-float device path (quantized/mod.rs:327-338), vulkan via the device
    // copy/cast path (vulkan_backend.rs:4324-4338), cpu natively.
    let dtype = weight.dtype();
    match dtype {
        GgmlDType::F32 | GgmlDType::F16 | GgmlDType::BF16 => {
            let device = weight.device();
            let w = weight.dequantize(&device)?.to_dtype(DType::F16)?;
            Ok(QMatMul::TensorF16(w))
        }
        _ => Ok(QMatMul::QTensor(weight)),
    }
}

fn get_qtensor_by_candidates(vb: &QuantizedVarBuilder, names: &[&str]) -> Result<Arc<QTensor>> {
    for name in names {
        if let Ok(tensor) = vb.get_no_shape(name) {
            return Ok(tensor);
        }
    }
    Err(candle_core::Error::msg(format!(
        "cannot find any GGUF tensor candidate: {}",
        names.join(", ")
    ))
    .into())
}

fn get_quantized_qtensor_by_candidates(
    vb: &QuantizedVarBuilder,
    names: &[&str],
) -> Result<Arc<QTensor>> {
    // Packed Q4/Q8 variants are required by Candle's CUDA quantized kernels.
    // Some GGUF tensor aliases resolve to float tensors; skip those in favor of
    // a packed candidate, but remember the first float alias as a fallback so
    // whole-file-float GGUFs (F32/BF16 base files) still load via
    // `keep_quantized_matmul`'s dense-matmul path.
    let mut float_fallback: Option<Arc<QTensor>> = None;
    for name in names {
        if let Ok(tensor) = vb.get_no_shape(name) {
            let dtype = tensor.dtype();
            if matches!(dtype, GgmlDType::F32 | GgmlDType::F16 | GgmlDType::BF16) {
                if float_fallback.is_none() {
                    float_fallback = Some(tensor);
                }
                continue;
            }
            return Ok(tensor);
        }
    }
    if let Some(tensor) = float_fallback {
        return Ok(tensor);
    }
    Err(candle_core::Error::msg(format!(
        "cannot find any quantized GGUF tensor candidate: {}",
        names.join(", ")
    ))
    .into())
}

impl Stage0Qwen3QuantizedBackbone {
    pub fn load(
        cfg: &Stage0Qwen3BackboneConfig,
        qvb: QuantizedVarBuilder,
        activation_dtype: DType,
    ) -> Result<Self> {
        let embed_tokens_q = get_qtensor_by_candidates(
            &qvb,
            &[
                "llm.embed_tokens.weight",
                "llm.embed_tokens",
                "llm.token_embd.weight",
                "llm.token_embd",
            ],
        )?;
        let embed_tokens =
            QuantizedEmbedding::new(embed_tokens_q, activation_dtype, qvb.device().clone())?;
        let rotary = Arc::new(RotaryEmbedding::new(cfg, qvb.device())?);
        let mut layers = Vec::with_capacity(cfg.num_hidden_layers);
        for index in 0..cfg.num_hidden_layers {
            let base = if qvb.contains_key(format!("llm.blk.{index}.attn_q.weight").as_str()) {
                qvb.pp(format!("llm.blk.{index}"))
            } else {
                qvb.pp(format!("llm.layers.{index}"))
            };
            layers.push(QwenDecoderLayer::load(
                cfg,
                rotary.clone(),
                base,
                activation_dtype,
            )?);
        }
        let norm = LocalRmsNorm::from_candidates(
            &qvb,
            &[
                "llm.output_norm.weight",
                "llm.output_norm",
                "llm.norm.weight",
                "llm.norm",
            ],
            cfg.rms_norm_eps,
            activation_dtype,
        )?;
        Ok(Self {
            embed_tokens,
            layers,
            norm,
        })
    }

    pub fn embed_text_tokens(&self, input_ids: &Tensor) -> Result<Tensor> {
        self.embed_tokens.forward(input_ids)
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
            if capture_layers.contains(&index) {
                captures.insert(index, hidden.clone());
            }
        }
        let final_hidden = self.norm.forward(&hidden)?;
        Ok(Stage0BackboneOutput {
            final_hidden,
            captured_hidden_layers: captures,
        })
    }
}
