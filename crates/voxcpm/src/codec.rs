//! Causal AudioVAE V2. Decoder state holds only receptive fields and overlap tails.
use crate::weights::Weights;
use candle_core::{DType, Result, Tensor};

struct Conv {
    weight: Tensor,
    bias: Tensor,
    stride: usize,
    dilation: usize,
    groups: usize,
    left: usize,
    history: Option<Tensor>,
}
impl Conv {
    fn load(
        w: &mut Weights,
        p: &str,
        stride: usize,
        dilation: usize,
        groups: usize,
        left: usize,
    ) -> Result<Self> {
        Ok(Self {
            weight: w.tensor(p)?,
            bias: w.tensor(&format!("{p}.bias"))?,
            stride,
            dilation,
            groups,
            left,
            history: None,
        })
    }
    fn reset(&mut self) {
        self.history = None;
    }
    fn forward(&mut self, x: &Tensor) -> Result<Tensor> {
        let (b, c, _) = x.dims3()?;
        let full = if self.left == 0 {
            x.clone()
        } else {
            let h = match &self.history {
                Some(h) => h.clone(),
                None => Tensor::zeros((b, c, self.left), x.dtype(), x.device())?,
            };
            let all = Tensor::cat(&[&h, x], 2)?;
            self.history = Some(
                all.narrow(2, all.dim(2)? - self.left, self.left)?
                    .contiguous()?,
            );
            all
        };
        let k = self.weight.dim(2)?;
        let y = if self.groups == c && self.weight.dim(0)? == c && self.weight.dim(1)? == 1 {
            let length = (full.dim(2)? - (k - 1) * self.dilation - 1) / self.stride + 1;
            if self.stride != 1 {
                candle_core::bail!("strided depthwise convolution is unsupported");
            }
            let mut y = Tensor::zeros((b, c, length), x.dtype(), x.device())?;
            for i in 0..k {
                let kernel = self.weight.narrow(2, i, 1)?.reshape((1, c, 1))?;
                y = (y + full
                    .narrow(2, i * self.dilation, length)?
                    .broadcast_mul(&kernel)?)?;
            }
            y
        } else {
            full.conv1d(&self.weight, 0, self.stride, self.dilation, self.groups)?
        };
        y.broadcast_add(&self.bias.reshape((1, (), 1))?)
    }
}
struct Upsample {
    weight: Tensor,
    bias: Tensor,
    stride: usize,
    tail: Option<Tensor>,
}
impl Upsample {
    fn load(w: &mut Weights, p: &str, stride: usize) -> Result<Self> {
        Ok(Self {
            weight: w.tensor(p)?,
            bias: w.tensor(&format!("{p}.bias"))?,
            stride,
            tail: None,
        })
    }
    fn forward(&mut self, x: &Tensor) -> Result<Tensor> {
        let (b, cin, n) = x.dims3()?;
        let cout = self.bias.elem_count();
        let s = self.stride;
        // Each input frame contributes two stride-sized blocks. The second is
        // retained and added to the next frame before applying the bias once.
        let products = x
            .transpose(1, 2)?
            .contiguous()?
            .reshape((b * n, cin))?
            .matmul(&self.weight.reshape((cin, cout * 2 * s))?)?
            .reshape((b, n, cout, 2 * s))?;
        let a = products
            .narrow(3, 0, s)?
            .transpose(1, 2)?
            .reshape((b, cout, n * s))?;
        let z = products
            .narrow(3, s, s)?
            .transpose(1, 2)?
            .reshape((b, cout, n * s))?;
        let tail = match &self.tail {
            Some(t) => t.clone(),
            None => Tensor::zeros((b, cout, s), x.dtype(), x.device())?,
        };
        let shifted = if n == 1 {
            tail
        } else {
            Tensor::cat(&[tail, z.narrow(2, 0, (n - 1) * s)?], 2)?
        };
        self.tail = Some(z.narrow(2, (n - 1) * s, s)?.contiguous()?);
        (a + shifted)?.broadcast_add(&self.bias.reshape((1, cout, 1))?)
    }
}
fn snake(x: &Tensor, alpha: &Tensor) -> Result<Tensor> {
    let dtype = x.dtype();
    let f = x.to_dtype(DType::F32)?;
    let a = alpha.to_dtype(DType::F32)?;
    (&f + f
        .broadcast_mul(&a)?
        .sin()?
        .sqr()?
        .broadcast_div(&(&a + 1e-9)?)?)?
    .to_dtype(dtype)
}
struct Residual {
    a: Tensor,
    b: Tensor,
    depthwise: Conv,
    pointwise: Conv,
}
impl Residual {
    fn load(w: &mut Weights, p: &str, c: usize, dilation: usize) -> Result<Self> {
        Ok(Self {
            a: w.tensor(&format!("{p}.block.0.alpha"))?,
            b: w.tensor(&format!("{p}.block.2.alpha"))?,
            depthwise: Conv::load(w, &format!("{p}.block.1"), 1, dilation, c, 6 * dilation)?,
            pointwise: Conv::load(w, &format!("{p}.block.3"), 1, 1, 1, 0)?,
        })
    }
    fn forward(&mut self, x: &Tensor) -> Result<Tensor> {
        let y = self.depthwise.forward(&snake(x, &self.a)?)?;
        x + self.pointwise.forward(&snake(&y, &self.b)?)?
    }
    fn reset(&mut self) {
        self.depthwise.reset();
        self.pointwise.reset();
    }
}
struct EncoderBlock {
    res: Vec<Residual>,
    alpha: Tensor,
    down: Conv,
}
struct DecoderBlock {
    res: Vec<Residual>,
    alpha: Tensor,
    up: Upsample,
    scale: Tensor,
    bias: Tensor,
}
pub(crate) struct Codec {
    first: Conv,
    enc: Vec<EncoderBlock>,
    mu: Conv,
    dec_first: Conv,
    dec_proj: Conv,
    dec: Vec<DecoderBlock>,
    last_alpha: Tensor,
    last: Conv,
}
impl Codec {
    pub fn load(w: &mut Weights) -> Result<Self> {
        let first = Conv::load(w, "audio_vae.encoder.block.0", 1, 1, 1, 6)?;
        let mut enc = Vec::new();
        let mut c = 128;
        for (i, s) in [2, 5, 8, 8].into_iter().enumerate() {
            let p = format!("audio_vae.encoder.block.{}", i + 1);
            let res = [1, 3, 9]
                .into_iter()
                .enumerate()
                .map(|(j, d)| Residual::load(w, &format!("{p}.block.{j}"), c, d))
                .collect::<Result<_>>()?;
            enc.push(EncoderBlock {
                res,
                alpha: w.tensor(&format!("{p}.block.3.alpha"))?,
                down: Conv::load(w, &format!("{p}.block.4"), s, 1, 1, s)?,
            });
            c *= 2;
        }
        let mu = Conv::load(w, "audio_vae.encoder.fc_mu", 1, 1, 1, 2)?;
        let dec_first = Conv::load(w, "audio_vae.decoder.model.0", 1, 1, 64, 6)?;
        let dec_proj = Conv::load(w, "audio_vae.decoder.model.1", 1, 1, 1, 0)?;
        let mut dec = Vec::new();
        let mut c = 2048;
        for (i, s) in [8, 6, 5, 2, 2, 2].into_iter().enumerate() {
            let index = i + 2;
            let p = format!("audio_vae.decoder.model.{index}");
            let res = [1, 3, 9]
                .into_iter()
                .enumerate()
                .map(|(j, d)| Residual::load(w, &format!("{p}.block.{}", j + 2), c / 2, d))
                .collect::<Result<_>>()?;
            let cond = format!("audio_vae.decoder.sr_cond_model.{index}");
            dec.push(DecoderBlock {
                res,
                alpha: w.tensor(&format!("{p}.block.0.alpha"))?,
                up: Upsample::load(w, &format!("{p}.block.1"), s)?,
                scale: w
                    .tensor(&format!("{cond}.scale_embed"))?
                    .narrow(0, 3, 1)?
                    .reshape((1, c, 1))?,
                bias: w
                    .tensor(&format!("{cond}.bias_embed"))?
                    .narrow(0, 3, 1)?
                    .reshape((1, c, 1))?,
            });
            c /= 2;
        }
        Ok(Self {
            first,
            enc,
            mu,
            dec_first,
            dec_proj,
            dec,
            last_alpha: w.tensor("audio_vae.decoder.model.8.alpha")?,
            last: Conv::load(w, "audio_vae.decoder.model.9", 1, 1, 1, 6)?,
        })
    }
    pub fn reset(&mut self) {
        self.first.reset();
        self.mu.reset();
        for b in &mut self.enc {
            b.down.reset();
            for r in &mut b.res {
                r.reset();
            }
        }
        self.dec_first.reset();
        self.dec_proj.reset();
        self.last.reset();
        for b in &mut self.dec {
            b.up.tail = None;
            for r in &mut b.res {
                r.reset();
            }
        }
    }
    pub fn encode(&mut self, x: &Tensor, cancel: &impl Fn() -> bool) -> Result<Tensor> {
        self.reset();
        let mut x = self.first.forward(x)?;
        for b in &mut self.enc {
            if cancel() {
                candle_core::bail!("generation cancelled");
            }
            for r in &mut b.res {
                x = r.forward(&x)?;
            }
            x = b.down.forward(&snake(&x, &b.alpha)?)?;
        }
        self.mu.forward(&x)
    }
    pub fn decode(&mut self, x: &Tensor, cancel: &impl Fn() -> bool) -> Result<Tensor> {
        let mut x = self.dec_proj.forward(&self.dec_first.forward(x)?)?;
        for b in &mut self.dec {
            if cancel() {
                candle_core::bail!("generation cancelled");
            }
            x = x.broadcast_mul(&b.scale)?.broadcast_add(&b.bias)?;
            x = b.up.forward(&snake(&x, &b.alpha)?)?;
            for r in &mut b.res {
                x = r.forward(&x)?;
            }
        }
        self.last.forward(&snake(&x, &self.last_alpha)?)?.tanh()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn tensor(fixture: &serde_json::Value, key: &str) -> Tensor {
        let v = &fixture[key];
        let shape: Vec<usize> = serde_json::from_value(v["shape"].clone()).unwrap();
        let data: Vec<f32> = serde_json::from_value(v["data"].clone()).unwrap();
        Tensor::from_vec(data, shape, &candle_core::Device::Cpu).unwrap()
    }
    fn close(actual: &Tensor, expected: &Tensor) {
        assert_eq!(actual.dims(), expected.dims());
        for (a, e) in actual
            .flatten_all()
            .unwrap()
            .to_vec1::<f32>()
            .unwrap()
            .iter()
            .zip(expected.flatten_all().unwrap().to_vec1::<f32>().unwrap())
        {
            assert!((*a - e).abs() <= 1e-5 + 1e-4 * e.abs(), "{a} != {e}");
        }
    }
    #[test]
    fn transposed_overlap_matches_torch_full_and_chunked() {
        let f: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/causal-convolutions.json"))
                .unwrap();
        let mut up = Upsample {
            weight: tensor(&f, "w"),
            bias: tensor(&f, "b"),
            stride: 3,
            tail: None,
        };
        let x = tensor(&f, "x");
        close(&up.forward(&x).unwrap(), &tensor(&f, "y"));
        up.tail = None;
        let chunks: Vec<_> = [(0, 1), (1, 2), (3, 4)]
            .into_iter()
            .map(|(start, len)| up.forward(&x.narrow(2, start, len).unwrap()).unwrap())
            .collect();
        close(&Tensor::cat(&chunks, 2).unwrap(), &tensor(&f, "y"));
        up.tail = None;
        close(&up.forward(&x).unwrap(), &tensor(&f, "y"));
    }
    #[test]
    fn causal_depthwise_matches_torch_across_boundaries() {
        let f: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/causal-convolutions.json"))
                .unwrap();
        let mut conv = Conv {
            weight: tensor(&f, "dw"),
            bias: tensor(&f, "db"),
            stride: 1,
            dilation: 3,
            groups: 2,
            left: 18,
            history: None,
        };
        let x = tensor(&f, "z");
        close(&conv.forward(&x).unwrap(), &tensor(&f, "d"));
        conv.reset();
        let chunks: Vec<_> = [(0, 2), (2, 9), (11, 8)]
            .into_iter()
            .map(|(start, len)| conv.forward(&x.narrow(2, start, len).unwrap()).unwrap())
            .collect();
        close(&Tensor::cat(&chunks, 2).unwrap(), &tensor(&f, "d"));
        assert_eq!(conv.history.as_ref().unwrap().dim(2).unwrap(), 18);
    }
}
