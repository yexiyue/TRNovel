//! Nano's fixed-seed inverse-CDF sampling, using the ONNX adapter's LCG.
use candle_core::{DType, Tensor};
use std::collections::HashSet;
pub(super) struct Sampler {
    state: u32,
}
impl Sampler {
    pub(super) fn new(seed: u64) -> Self {
        Self { state: seed as u32 }
    }
    pub(super) fn sample(
        &mut self,
        logits: &Tensor,
        history: &[u32],
        temperature: f32,
        top_k: usize,
        top_p: f64,
        penalty: f32,
    ) -> anyhow::Result<u32> {
        let mut scores = logits
            .flatten_all()?
            .to_dtype(DType::F32)?
            .to_vec1::<f32>()?;
        anyhow::ensure!(
            !scores.is_empty() && scores.iter().all(|v| v.is_finite()),
            "invalid Nano logits"
        );
        for id in history.iter().copied().collect::<HashSet<_>>() {
            let value = scores
                .get_mut(id as usize)
                .ok_or_else(|| anyhow::anyhow!("invalid Nano repetition token"))?;
            *value = if *value < 0. {
                *value * penalty
            } else {
                *value / penalty
            };
        }
        for score in &mut scores {
            *score /= temperature;
        }
        let mut ranked: Vec<_> = scores.iter().copied().enumerate().collect();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1));
        let threshold = ranked[top_k.min(ranked.len()) - 1].1;
        let maximum = ranked[0].1;
        let mut weights: Vec<f64> = scores
            .iter()
            .map(|v| {
                if *v < threshold {
                    0.
                } else {
                    ((*v - maximum) as f64).exp()
                }
            })
            .collect();
        if top_p < 1. {
            let total: f64 = weights.iter().sum();
            let mut cumulative = 0.;
            for (id, _) in ranked {
                if cumulative > top_p {
                    weights[id] = 0.;
                } else {
                    cumulative += weights[id] / total;
                }
            }
        }
        self.state = self.state.wrapping_mul(1664525).wrapping_add(1013904223);
        let mut draw = (self.state >> 8) as f64 / 16777216. * weights.iter().sum::<f64>();
        for (id, weight) in weights.iter().enumerate() {
            draw -= weight;
            if *weight > 0. && draw <= 0. {
                return Ok(id as u32);
            }
        }
        anyhow::bail!("invalid Nano sampling distribution")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::Device;
    #[test]
    fn fixed_draws_preserve_token_order_nucleus_and_repetition() -> anyhow::Result<()> {
        let logits = Tensor::new(&[0f32, 1., 2., 3.], &Device::Cpu)?;
        // Seed 42's first LCG draw is 0.252345..., so inverse CDF in token
        // order chooses token 2 (sampling in rank order would choose token 3).
        assert_eq!(Sampler::new(42).sample(&logits, &[], 1., 4, 1., 1.)?, 2);
        assert_eq!(Sampler::new(42).sample(&logits, &[], 1., 4, 0.5, 1.)?, 3);
        assert_eq!(
            Sampler::new(42).sample(&logits, &[3, 3, 3], 1., 1, 1., 2.)?,
            2
        );
        assert!(
            Sampler::new(42)
                .sample(&Tensor::new(&[f32::NAN], &Device::Cpu)?, &[], 1., 1, 1., 1.)
                .is_err()
        );
        Ok(())
    }
}
