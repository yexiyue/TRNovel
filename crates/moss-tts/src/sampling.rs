//! Upstream temperature, repetition penalty, top-k and nucleus sampling.
use candle_core::{DType, Tensor};
use rand::{Rng, rngs::StdRng};
use std::collections::HashSet;

#[derive(Clone, Copy)]
pub(crate) struct Parameters {
    pub temperature: f32,
    pub top_k: usize,
    pub top_p: f64,
    pub penalty: f32,
}
impl Parameters {
    pub(crate) const TEXT: Self = Self {
        temperature: 1.5,
        top_k: 50,
        top_p: 1.,
        penalty: 1.,
    };
    pub(crate) const LOCAL: Self = Self {
        temperature: 1.,
        top_k: 50,
        top_p: 0.95,
        penalty: 1.1,
    };
}

pub(crate) fn sample(
    logits: &Tensor,
    history: &[u32],
    parameters: Parameters,
    exclude: Option<u32>,
    rng: &mut StdRng,
) -> anyhow::Result<u32> {
    let Parameters {
        temperature,
        top_k,
        top_p,
        penalty,
    } = parameters;
    let mut values = logits
        .flatten_all()?
        .to_dtype(DType::F32)?
        .to_vec1::<f32>()?;
    for id in history.iter().copied().collect::<HashSet<_>>() {
        if let Some(value) = values.get_mut(id as usize) {
            *value = if *value < 0. {
                *value * penalty
            } else {
                *value / penalty
            };
        }
    }
    if let Some(id) = exclude {
        values[id as usize] = f32::NEG_INFINITY;
    }
    anyhow::ensure!(
        values.iter().any(|v| v.is_finite()) && values.iter().all(|v| !v.is_nan()),
        "invalid MOSS logits"
    );
    let mut ranked: Vec<_> = values.into_iter().enumerate().collect();
    ranked.sort_unstable_by(|a, b| b.1.total_cmp(&a.1));
    if temperature == 0. {
        return Ok(ranked[0].0 as u32);
    }
    ranked.truncate(top_k.min(ranked.len()));
    let maximum = ranked[0].1;
    let mut weights: Vec<_> = ranked
        .iter()
        .map(|(_, value)| ((*value - maximum) / temperature).exp() as f64)
        .collect();
    let total: f64 = weights.iter().sum();
    let mut cumulative = 0.;
    let mut keep = weights.len();
    for (i, weight) in weights.iter().enumerate() {
        cumulative += weight / total;
        if cumulative > top_p {
            keep = i + 1;
            break;
        }
    }
    weights.truncate(keep);
    let mut draw = rng.random::<f64>() * weights.iter().sum::<f64>();
    for (i, weight) in weights.iter().enumerate() {
        draw -= weight;
        if draw <= 0. {
            return Ok(ranked[i].0 as u32);
        }
    }
    Ok(ranked[keep - 1].0 as u32)
}
