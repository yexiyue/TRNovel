//! GGUF tensors retain their production names and quantization.
use candle_core::{
    DType, Device, Result, Tensor,
    quantized::{
        GgmlDType, QMatMul,
        gguf_file::{Content, Value},
    },
};
use std::{fs::File, path::Path};

enum Source {
    Gguf { content: Content, file: File },
    Native(candle_nn::VarBuilder<'static>),
}

pub(crate) struct Weights<'a> {
    source: Source,
    pub device: Device,
    pub dtype: DType,
    pub quantized: bool,
    cancel: Option<&'a dyn Fn() -> bool>,
}

impl<'a> Weights<'a> {
    pub fn open(path: &Path, device: &Device, dtype: DType) -> Result<Self> {
        let mut file = File::open(path)?;
        let content = Content::read(&mut file)?;
        Ok(Self {
            source: Source::Gguf { content, file },
            device: device.clone(),
            dtype,
            quantized: true,
            cancel: None,
        })
    }
    pub fn native(path: &Path, device: &Device, dtype: DType) -> Result<Self> {
        let builder = if path.extension().is_some_and(|s| s == "safetensors") {
            // Files are immutable pinned assets; mappings live through initialization.
            unsafe { candle_nn::VarBuilder::from_mmaped_safetensors(&[path], dtype, device)? }
        } else {
            candle_nn::VarBuilder::from_pth_with_state(path, dtype, "state_dict", device)?
        };
        Ok(Self {
            source: Source::Native(builder),
            device: device.clone(),
            dtype,
            quantized: false,
            cancel: None,
        })
    }
    pub fn with_cancel(mut self, cancel: &'a dyn Fn() -> bool) -> Self {
        self.cancel = Some(cancel);
        self
    }
    fn check_cancel(&self) -> Result<()> {
        if self.cancel.is_some_and(|c| c()) {
            candle_core::bail!("model loading cancelled");
        }
        Ok(())
    }
    pub fn tensor(&mut self, name: &str) -> Result<Tensor> {
        self.check_cancel()?;
        match &mut self.source {
            Source::Gguf { content, file } => content
                .tensor(file, name, &self.device)?
                .dequantize(&self.device)?
                .to_dtype(self.dtype),
            Source::Native(builder) => {
                if let Some(key) = name.strip_prefix("audio_vae.") {
                    if builder.contains_tensor(key) {
                        return builder.get_unchecked(key);
                    }
                    if builder.contains_tensor(&format!("{key}.weight")) {
                        return builder.get_unchecked(&format!("{key}.weight"));
                    }
                    let v = builder.get_unchecked_dtype(&format!("{key}.weight_v"), DType::F32)?;
                    let g = builder.get_unchecked_dtype(&format!("{key}.weight_g"), DType::F32)?;
                    return fold_weight_norm(&g, &v)?.to_dtype(self.dtype);
                }
                builder.get_unchecked(&native_name(name)?)
            }
        }
    }
    pub fn linear(&mut self, name: &str, bias: bool) -> Result<Linear> {
        self.check_cancel()?;
        let mat = match &mut self.source {
            Source::Gguf { content, file } => {
                let q = content.tensor(file, &format!("{name}.weight"), &self.device)?;
                if self.quantized && q.dtype() == GgmlDType::Q8_0 {
                    QMatMul::QTensor(std::sync::Arc::new(q))
                } else {
                    QMatMul::Tensor(q.dequantize(&self.device)?.to_dtype(self.dtype)?)
                }
            }
            Source::Native(_) => QMatMul::Tensor(self.tensor(&format!("{name}.weight"))?),
        };
        let bias = if bias {
            Some(self.tensor(&format!("{name}.bias"))?)
        } else {
            None
        };
        Ok(Linear { mat, bias })
    }
    pub fn value(&self, name: &str) -> Result<&Value> {
        match &self.source {
            Source::Gguf { content, .. } => content
                .metadata
                .get(name)
                .ok_or_else(|| candle_core::Error::Msg(format!("missing GGUF metadata {name}"))),
            Source::Native(_) => candle_core::bail!("metadata is unavailable for native weights"),
        }
    }
    pub fn expect_u32(&self, name: &str, expected: u32) -> Result<()> {
        let actual = self.value(name)?.to_u32()?;
        if actual != expected {
            candle_core::bail!("unsupported GGUF {name}={actual}; expected {expected}");
        }
        Ok(())
    }
    pub fn expect_string(&self, name: &str, expected: &str) -> Result<()> {
        let actual = self.value(name)?.to_string()?;
        if actual != expected {
            candle_core::bail!("unsupported GGUF {name}={actual}; expected {expected}");
        }
        Ok(())
    }
}

fn fold_weight_norm(g: &Tensor, v: &Tensor) -> Result<Tensor> {
    let axes: Vec<usize> = (1..v.rank()).collect();
    v.broadcast_mul(g)?
        .broadcast_div(&v.sqr()?.sum_keepdim(axes)?.sqrt()?)
}

/// Alias production tensor names to the original OpenBMB state dictionary.
fn native_name(name: &str) -> Result<String> {
    for (prefix, native) in [
        ("residual_lm.", "residual_lm."),
        ("locenc.", "feat_encoder.encoder."),
        ("locdit.", "feat_decoder.estimator.decoder."),
        ("", "base_lm."),
    ] {
        if let Some(rest) = name
            .strip_prefix(prefix)
            .and_then(|s| s.strip_prefix("blk."))
        {
            let (layer, tensor) = rest
                .split_once('.')
                .ok_or_else(|| candle_core::Error::Msg("invalid layer tensor".into()))?;
            let (tensor, suffix) = tensor
                .rsplit_once('.')
                .ok_or_else(|| candle_core::Error::Msg("invalid tensor suffix".into()))?;
            let mapped = match tensor {
                "attn_norm" => "input_layernorm",
                "ffn_norm" => "post_attention_layernorm",
                "attn_q" => "self_attn.q_proj",
                "attn_k" => "self_attn.k_proj",
                "attn_v" => "self_attn.v_proj",
                "attn_output" => "self_attn.o_proj",
                "ffn_gate" => "mlp.gate_proj",
                "ffn_up" => "mlp.up_proj",
                "ffn_down" => "mlp.down_proj",
                _ => candle_core::bail!("unsupported layer tensor {name}"),
            };
            return Ok(format!("{native}layers.{layer}.{mapped}.{suffix}"));
        }
    }
    let mapped = match name {
        "token_embd.weight" => "base_lm.embed_tokens.weight",
        "output_norm.weight" => "base_lm.norm.weight",
        "residual_lm.output_norm.weight" => "residual_lm.norm.weight",
        "locenc.norm.weight" => "feat_encoder.encoder.norm.weight",
        "locenc.cls_token.weight" => "feat_encoder.special_token",
        "locdit.norm.weight" => "feat_decoder.estimator.decoder.norm.weight",
        _ => {
            for (prefix, native) in [
                ("locenc.", "feat_encoder."),
                ("locdit.", "feat_decoder.estimator."),
                ("fsq.", "fsq_layer."),
                ("projections.res_fusion_proj.", "fusion_concat_proj."),
                ("projections.", ""),
                ("stop_predictor.linear1.", "stop_proj."),
                ("stop_predictor.linear2.", "stop_head."),
            ] {
                if let Some(rest) = name.strip_prefix(prefix) {
                    return Ok(format!("{native}{rest}"));
                }
            }
            candle_core::bail!("unsupported original tensor {name}");
        }
    };
    Ok(mapped.to_owned())
}

pub(crate) struct Linear {
    pub mat: QMatMul,
    pub bias: Option<Tensor>,
}
impl Linear {
    pub fn forward(&self, x: &Tensor) -> Result<Tensor> {
        use candle_core::Module;
        let mut shape = x.dims().to_vec();
        let input = *shape
            .last()
            .ok_or_else(|| candle_core::Error::Msg("linear input must have dimensions".into()))?;
        let rows = x.elem_count() / input;
        let y = self.mat.forward(&x.contiguous()?.reshape((rows, input))?)?;
        *shape.last_mut().expect("checked nonempty shape") = y.dim(1)?;
        let y = y.reshape(shape)?;
        match &self.bias {
            Some(b) => y.broadcast_add(b),
            None => Ok(y),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn weight_norm_normalizes_each_output_channel() {
        let v = Tensor::new(&[[[3f32, 4.]], [[0., 2.]]], &Device::Cpu).unwrap();
        let g = Tensor::new(&[[[10f32]], [[3.]]], &Device::Cpu).unwrap();
        assert_eq!(
            fold_weight_norm(&g, &v)
                .unwrap()
                .flatten_all()
                .unwrap()
                .to_vec1::<f32>()
                .unwrap(),
            vec![6., 8., 0., 3.]
        );
    }
    #[test]
    fn original_tensor_aliases_cover_each_network() {
        for (name, original) in [
            (
                "blk.27.attn_q.weight",
                "base_lm.layers.27.self_attn.q_proj.weight",
            ),
            (
                "residual_lm.blk.7.ffn_gate.weight",
                "residual_lm.layers.7.mlp.gate_proj.weight",
            ),
            (
                "locenc.blk.11.attn_norm.weight",
                "feat_encoder.encoder.layers.11.input_layernorm.weight",
            ),
            (
                "locdit.blk.11.attn_output.weight",
                "feat_decoder.estimator.decoder.layers.11.self_attn.o_proj.weight",
            ),
            ("locenc.cls_token.weight", "feat_encoder.special_token"),
            (
                "projections.res_fusion_proj.bias",
                "fusion_concat_proj.bias",
            ),
            ("stop_predictor.linear2.weight", "stop_head.weight"),
        ] {
            assert_eq!(native_name(name).unwrap(), original);
        }
        assert!(native_name("unknown.weight").is_err());
    }
}
