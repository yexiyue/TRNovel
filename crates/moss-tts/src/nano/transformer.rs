//! GPT2 with biased projections, interleaved RoPE and GELU-new.
use candle_core::{Module, Tensor};
use candle_nn::{LayerNorm, Linear, VarBuilder, layer_norm, linear};
use serde::Deserialize;
#[derive(Clone, Deserialize)]
pub(super) struct Config {
    pub vocab_size: usize,
    pub n_embd: usize,
    pub n_head: usize,
    pub n_layer: usize,
    pub n_inner: Option<usize>,
    pub layer_norm_epsilon: f64,
    pub activation_function: String,
    pub rope_base: f64,
}
struct Layer {
    norm1: LayerNorm,
    norm2: LayerNorm,
    qkv: Linear,
    output: Linear,
    up: Linear,
    down: Linear,
    cache: Option<(Tensor, Tensor)>,
}
pub(super) struct Transformer {
    layers: Vec<Layer>,
    norm: LayerNorm,
    config: Config,
    offset: usize,
}
impl Transformer {
    pub(super) fn load(c: &Config, vb: VarBuilder<'_>) -> anyhow::Result<Self> {
        anyhow::ensure!(
            c.n_head > 0
                && c.n_embd.is_multiple_of(c.n_head)
                && (c.n_embd / c.n_head).is_multiple_of(2)
                && c.activation_function == "gelu_new",
            "unsupported Nano transformer"
        );
        let d = c.n_embd;
        let inner = c.n_inner.unwrap_or(4 * d);
        let layers = (0..c.n_layer)
            .map(|i| {
                let vb = vb.pp(format!("h.{i}"));
                Ok(Layer {
                    norm1: layer_norm(d, c.layer_norm_epsilon, vb.pp("ln_1"))?,
                    norm2: layer_norm(d, c.layer_norm_epsilon, vb.pp("ln_2"))?,
                    qkv: linear(d, 3 * d, vb.pp("attn.c_attn"))?,
                    output: linear(d, d, vb.pp("attn.c_proj"))?,
                    up: linear(d, inner, vb.pp("mlp.fc_in"))?,
                    down: linear(inner, d, vb.pp("mlp.fc_out"))?,
                    cache: None,
                })
            })
            .collect::<candle_core::Result<Vec<_>>>()?;
        Ok(Self {
            layers,
            norm: layer_norm(d, c.layer_norm_epsilon, vb.pp("ln_f"))?,
            config: c.clone(),
            offset: 0,
        })
    }
    pub(super) fn reset(&mut self) {
        self.offset = 0;
        for layer in &mut self.layers {
            layer.cache = None;
        }
    }
    pub(super) fn forward(
        &mut self,
        input: &Tensor,
        cancelled: &impl Fn() -> bool,
    ) -> anyhow::Result<Tensor> {
        let (b, t, d) = input.dims3()?;
        let heads = self.config.n_head;
        let mut x = input.clone();
        for layer in &mut self.layers {
            crate::check_cancel(cancelled)?;
            let qkv = layer.qkv.forward(&layer.norm1.forward(&x)?)?;
            let shape = (b, t, heads, d / heads);
            let q = crate::transformer::rope(
                &qkv.narrow(2, 0, d)?.reshape(shape)?.transpose(1, 2)?,
                self.offset,
                self.config.rope_base,
                true,
            )?;
            let k = crate::transformer::rope(
                &qkv.narrow(2, d, d)?.reshape(shape)?.transpose(1, 2)?,
                self.offset,
                self.config.rope_base,
                true,
            )?;
            let v = qkv.narrow(2, 2 * d, d)?.reshape(shape)?.transpose(1, 2)?;
            let (k, v) = match &layer.cache {
                Some((pk, pv)) => (Tensor::cat(&[pk, &k], 2)?, Tensor::cat(&[pv, &v], 2)?),
                None => (k, v),
            };
            let attended = crate::transformer::attend(&q, &k, &v, self.offset, 0, None)?
                .transpose(1, 2)?
                .reshape((b, t, d))?;
            layer.cache = Some((k, v));
            x = (&x + layer.output.forward(&attended)?)?;
            let up = layer.up.forward(&layer.norm2.forward(&x)?)?;
            // Candle's GELU uses the GPT2 tanh approximation (gelu_new).
            x = (&x + layer.down.forward(&up.gelu()?)?)?;
        }
        self.offset += t;
        Ok(self.norm.forward(&x)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::{DType, Device};
    #[test]
    fn pytorch_reference_matches_prefill_cached_decode_and_cancel() -> anyhow::Result<()> {
        let reference: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/nano-transformer.json"))?;
        let config: Config = serde_json::from_value(reference["config"].clone())?;
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/nano-transformer.safetensors");
        let device = Device::Cpu;
        // SAFETY: checked-in reference weights are immutable for the duration of the test.
        let vb = unsafe { VarBuilder::from_mmaped_safetensors(&[path], DType::F32, &device)? };
        let mut model = Transformer::load(&config, vb)?;
        let input: Vec<f32> = serde_json::from_value(reference["input"].clone())?;
        let expected: Vec<f32> = serde_json::from_value(reference["output"].clone())?;
        let input = Tensor::from_vec(input, (1, 5, 16), &device)?;
        let assert_matches = |output: Tensor| -> anyhow::Result<()> {
            let output = output.flatten_all()?.to_vec1::<f32>()?;
            let error = output
                .iter()
                .zip(&expected)
                .map(|(a, b)| (a - b).abs())
                .fold(0f32, f32::max);
            assert!(error < 2e-5, "Nano GPT2 numerical error: {error}");
            Ok(())
        };
        assert_matches(model.forward(&input, &|| false)?)?;
        model.reset();
        let first = model.forward(&input.narrow(1, 0, 3)?, &|| false)?;
        let rest = model.forward(&input.narrow(1, 3, 2)?, &|| false)?;
        assert_matches(Tensor::cat(&[first, rest], 1)?)?;
        model.reset();
        assert!(model.forward(&input, &|| true).is_err());
        Ok(())
    }
}
