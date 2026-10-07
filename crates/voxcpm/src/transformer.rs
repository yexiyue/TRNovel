//! MiniCPM4 shared by the global, residual and local networks.
use crate::weights::{Linear, Weights};
use candle_core::{DType, Result, Tensor};

struct Layer {
    norm: Tensor,
    q: Linear,
    k: Linear,
    v: Linear,
    o: Linear,
    ffn_norm: Tensor,
    gate: Linear,
    up: Linear,
    down: Linear,
}
pub(crate) struct Transformer {
    layers: Vec<Layer>,
    norm: Tensor,
    rope: Option<Rope>,
    causal: bool,
    eps: f64,
}
pub(crate) struct Cache {
    kv: Vec<Option<(Tensor, Tensor)>>,
    pub position: usize,
    limit: usize,
}
impl Cache {
    pub fn new(layers: usize, limit: usize) -> Self {
        Self {
            kv: vec![None; layers],
            position: 0,
            limit,
        }
    }
}

fn norm(x: &Tensor, w: &Tensor, eps: f64) -> Result<Tensor> {
    candle_nn::ops::rms_norm(&x.contiguous()?, w, eps as f32)
}
struct Rope {
    cos: Tensor,
    sin: Tensor,
}
impl Rope {
    fn new(factors: &Tensor) -> Result<Self> {
        let freq: Vec<f32> = (0..64)
            .map(|i| 10000f64.powf(-((2 * i) as f64) / 128.0) as f32)
            .collect();
        let f = Tensor::from_vec(freq, (1, 64), factors.device())?
            .broadcast_div(&factors.to_dtype(DType::F32)?.reshape((1, 64))?)?;
        let positions = Tensor::arange(0u32, 4096u32, factors.device())?
            .to_dtype(DType::F32)?
            .unsqueeze(1)?;
        let angles = positions.matmul(&f)?;
        Ok(Self {
            cos: angles.cos()?,
            sin: angles.sin()?,
        })
    }
}
impl Transformer {
    pub fn load(
        w: &mut Weights,
        prefix: &str,
        count: usize,
        norm_name: &str,
        rope: Option<Tensor>,
        causal: bool,
    ) -> Result<Self> {
        let mut layers = Vec::with_capacity(count);
        for i in 0..count {
            let p = format!("{prefix}blk.{i}");
            layers.push(Layer {
                norm: w.tensor(&format!("{p}.attn_norm.weight"))?,
                q: w.linear(&format!("{p}.attn_q"), false)?,
                k: w.linear(&format!("{p}.attn_k"), false)?,
                v: w.linear(&format!("{p}.attn_v"), false)?,
                o: w.linear(&format!("{p}.attn_output"), false)?,
                ffn_norm: w.tensor(&format!("{p}.ffn_norm.weight"))?,
                gate: w.linear(&format!("{p}.ffn_gate"), false)?,
                up: w.linear(&format!("{p}.ffn_up"), false)?,
                down: w.linear(&format!("{p}.ffn_down"), false)?,
            });
        }
        Ok(Self {
            layers,
            norm: w.tensor(norm_name)?,
            rope: rope.as_ref().map(Rope::new).transpose()?,
            causal,
            eps: 1e-5,
        })
    }
    pub fn cache(&self, limit: usize) -> Cache {
        Cache::new(self.layers.len(), limit)
    }
    pub fn forward(
        &self,
        input: &Tensor,
        mut cache: Option<&mut Cache>,
        cancel: &impl Fn() -> bool,
    ) -> Result<Tensor> {
        let (b, t, _) = input.dims3()?;
        let offset = cache.as_ref().map_or(0, |c| c.position);
        if cache.as_ref().is_some_and(|c| offset + t > c.limit) {
            candle_core::bail!("Transformer context limit exceeded");
        }
        let mut x = input.clone();
        for (index, l) in self.layers.iter().enumerate() {
            if cancel() {
                candle_core::bail!("generation cancelled");
            }
            let h = norm(&x, &l.norm, self.eps)?;
            let q = l.q.forward(&h)?.reshape((b, t, 16, 128))?.transpose(1, 2)?;
            let k = l.k.forward(&h)?.reshape((b, t, 2, 128))?.transpose(1, 2)?;
            let v = l.v.forward(&h)?.reshape((b, t, 2, 128))?.transpose(1, 2)?;
            let (q, k) = match &self.rope {
                Some(factors) => (rotate(&q, offset, factors)?, rotate(&k, offset, factors)?),
                None => (q, k),
            };
            let (k, v) = if let Some(c) = cache.as_mut() {
                let pair = match &c.kv[index] {
                    Some((oldk, oldv)) => {
                        (Tensor::cat(&[oldk, &k], 2)?, Tensor::cat(&[oldv, &v], 2)?)
                    }
                    None => (k, v),
                };
                c.kv[index] = Some(pair.clone());
                pair
            } else {
                (k, v)
            };
            let len = k.dim(2)?;
            let k = k
                .unsqueeze(2)?
                .broadcast_as((b, 2, 8, len, 128))?
                .reshape((b, 16, len, 128))?;
            let v = v
                .unsqueeze(2)?
                .broadcast_as((b, 2, 8, len, 128))?
                .reshape((b, 16, len, 128))?;
            let mut scores = (q
                .contiguous()?
                .matmul(&k.transpose(2, 3)?.contiguous()?)?
                .to_dtype(DType::F32)?
                / 128f64.sqrt())?;
            if self.causal && t > 1 {
                let mask: Vec<u8> = (0..t)
                    .flat_map(|i| (0..len).map(move |j| u8::from(j > offset + i)))
                    .collect();
                let mask = Tensor::from_vec(mask, (1, 1, t, len), x.device())?
                    .broadcast_as(scores.shape())?;
                scores = mask.where_cond(
                    &Tensor::full(f32::NEG_INFINITY, scores.shape(), x.device())?,
                    &scores,
                )?;
            }
            let attended = candle_nn::ops::softmax_last_dim(&scores)?
                .to_dtype(x.dtype())?
                .matmul(&v.contiguous()?)?
                .transpose(1, 2)?
                .reshape((b, t, 2048))?;
            x = (x + l.o.forward(&attended)?)?;
            let h = norm(&x, &l.ffn_norm, self.eps)?;
            x = (x + l
                .down
                .forward(&(l.gate.forward(&h)?.silu()? * l.up.forward(&h)?)?)?)?;
        }
        if let Some(c) = cache {
            c.position += t;
        }
        norm(&x, &self.norm, self.eps)
    }
}
fn rotate(x: &Tensor, offset: usize, rope: &Rope) -> Result<Tensor> {
    let (_, _, t, d) = x.dims4()?;
    let cos = rope.cos.narrow(0, offset, t)?.unsqueeze(0)?.unsqueeze(0)?;
    let sin = rope.sin.narrow(0, offset, t)?.unsqueeze(0)?.unsqueeze(0)?;
    let f = x.to_dtype(DType::F32)?;
    let first = f.narrow(3, 0, d / 2)?;
    let second = f.narrow(3, d / 2, d / 2)?;
    let left = (first.broadcast_mul(&cos)? - second.broadcast_mul(&sin)?)?;
    let right = (second.broadcast_mul(&cos)? + first.broadcast_mul(&sin)?)?;
    Tensor::cat(&[left, right], 3)?.to_dtype(x.dtype())
}
