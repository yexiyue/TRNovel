//! Local feature encoder, FSQ and conditional flow matching.
use crate::{
    transformer::Transformer,
    weights::{Linear, Weights},
};
use candle_core::{DType, Result, Tensor};

struct Schedule {
    grid: Vec<f32>,
    times: Vec<Tensor>,
}

pub(crate) struct Acoustic {
    encoder: Transformer,
    encoder_in: Linear,
    cls: Tensor,
    enc_to_lm: Linear,
    fsq_in: Linear,
    fsq_out: Linear,
    dit: Transformer,
    dit_in: Linear,
    dit_cond: Linear,
    dit_out: Linear,
    time1: Linear,
    time2: Linear,
    delta1: Linear,
    delta2: Linear,
    pub lm_to_dit: Linear,
    pub res_to_dit: Linear,
    pub fusion: Linear,
    stop1: Linear,
    stop2: Linear,
    schedule: Option<Schedule>,
}
impl Acoustic {
    pub fn load(w: &mut Weights, rope: &Tensor) -> Result<Self> {
        Ok(Self {
            encoder: Transformer::load(
                w,
                "locenc.",
                12,
                "locenc.norm.weight",
                Some(rope.clone()),
                false,
            )?,
            encoder_in: w.linear("locenc.in_proj", true)?,
            cls: w.tensor("locenc.cls_token.weight")?,
            enc_to_lm: w.linear("projections.enc_to_lm_proj", true)?,
            fsq_in: w.linear("fsq.in_proj", true)?,
            fsq_out: w.linear("fsq.out_proj", true)?,
            dit: Transformer::load(
                w,
                "locdit.",
                12,
                "locdit.norm.weight",
                Some(rope.clone()),
                false,
            )?,
            dit_in: w.linear("locdit.in_proj", true)?,
            dit_cond: w.linear("locdit.cond_proj", true)?,
            dit_out: w.linear("locdit.out_proj", true)?,
            time1: w.linear("locdit.time_mlp.linear_1", true)?,
            time2: w.linear("locdit.time_mlp.linear_2", true)?,
            delta1: w.linear("locdit.delta_time_mlp.linear_1", true)?,
            delta2: w.linear("locdit.delta_time_mlp.linear_2", true)?,
            lm_to_dit: w.linear("projections.lm_to_dit_proj", true)?,
            res_to_dit: w.linear("projections.res_to_dit_proj", true)?,
            fusion: w.linear("projections.res_fusion_proj", true)?,
            stop1: w.linear("stop_predictor.linear1", true)?,
            stop2: w.linear("stop_predictor.linear2", false)?,
            schedule: None,
        })
    }
    pub fn encode(&self, features: &Tensor, cancel: &impl Fn() -> bool) -> Result<Tensor> {
        let b = features.dim(0)?;
        let projected = self.encoder_in.forward(features)?;
        let cls = self.cls.reshape((1, 1, 1024))?.broadcast_as((b, 1, 1024))?;
        self.enc_to_lm.forward(
            &self
                .encoder
                .forward(&Tensor::cat(&[cls, projected], 1)?, None, cancel)?
                .narrow(1, 0, 1)?
                .squeeze(1)?,
        )
    }
    pub fn fsq(&self, x: &Tensor) -> Result<Tensor> {
        let hidden = (self.fsq_in.forward(x)?.to_dtype(DType::F32)?.tanh()? * 9.0)?;
        // Explicit ties-to-even is validated separately from F16/quantized error.
        let rounded = round_ties_even(&hidden)?;
        self.fsq_out.forward(&(rounded / 9.0)?.to_dtype(x.dtype())?)
    }
    pub fn stop(&self, lm: &Tensor) -> Result<bool> {
        let logits = self
            .stop2
            .forward(&self.stop1.forward(lm)?.silu()?)?
            .to_dtype(DType::F32)?
            .flatten_all()?
            .to_vec1::<f32>()?;
        Ok(logits[1] > logits[0])
    }
    fn time(&self, t: f32, like: &Tensor) -> Result<Tensor> {
        let half = 512;
        let mut values = Vec::with_capacity(1024);
        for i in 0..half {
            values.push((1000.0 * t * (-(i as f32) * 10000f32.ln() / 511.0).exp()).sin());
        }
        for i in 0..half {
            values.push((1000.0 * t * (-(i as f32) * 10000f32.ln() / 511.0).exp()).cos());
        }
        Tensor::from_vec(values, (1, 1024), like.device())?.to_dtype(like.dtype())
    }
    #[cfg(test)]
    pub(crate) fn velocity(
        &self,
        x: &Tensor,
        mu: &Tensor,
        cond: &Tensor,
        t: f32,
        cancel: &impl Fn() -> bool,
    ) -> Result<Tensor> {
        let sequence = self.velocity_tokens(x, mu, cond, t)?;
        self.dit_out
            .forward(&self.dit.forward(&sequence, None, cancel)?.narrow(1, 7, 4)?)?
            .transpose(1, 2)?
            .to_dtype(DType::F32)
    }
    #[cfg(test)]
    pub(crate) fn velocity_tokens(
        &self,
        x: &Tensor,
        mu: &Tensor,
        cond: &Tensor,
        t: f32,
    ) -> Result<Tensor> {
        let cond = self.dit_cond.forward(&cond.transpose(1, 2)?)?;
        let time = (self
            .time2
            .forward(&self.time1.forward(&self.time(t, mu)?)?.silu()?)?
            + self
                .delta2
                .forward(&self.delta1.forward(&self.time(0.0, mu)?)?.silu()?)?)?
        .unsqueeze(1)?;
        self.prepared_tokens(x, mu, &cond, &time)
    }
    fn prepared_tokens(
        &self,
        x: &Tensor,
        mu: &Tensor,
        cond: &Tensor,
        time: &Tensor,
    ) -> Result<Tensor> {
        let b = mu.dim(0)?;
        let input = self
            .dit_in
            .forward(&x.transpose(1, 2)?.to_dtype(mu.dtype())?)?;
        Tensor::cat(
            &[
                mu.reshape((b, 2, 1024))?,
                time.broadcast_as((b, 1, 1024))?,
                cond.clone(),
                input,
            ],
            1,
        )
    }
    fn prepare_schedule(
        &mut self,
        steps: usize,
        like: &Tensor,
        cancel: &impl Fn() -> bool,
    ) -> Result<()> {
        if self
            .schedule
            .as_ref()
            .is_some_and(|s| s.times.len() == steps)
        {
            return Ok(());
        }
        let grid: Vec<f32> = (0..=steps)
            .map(|i| {
                let t = 1.0 - i as f32 / steps as f32;
                2.0 * t + (std::f32::consts::FRAC_PI_2 * t).cos() - 1.0
            })
            .collect();
        let delta = self
            .delta2
            .forward(&self.delta1.forward(&self.time(0.0, like)?)?.silu()?)?;
        let mut times = Vec::with_capacity(steps);
        for &t in &grid[..steps] {
            if cancel() {
                candle_core::bail!("generation cancelled");
            }
            times.push(
                (self
                    .time2
                    .forward(&self.time1.forward(&self.time(t, like)?)?.silu()?)?
                    + &delta)?
                    .unsqueeze(1)?,
            );
        }
        self.schedule = Some(Schedule { grid, times });
        Ok(())
    }
    pub fn sample(
        &mut self,
        mu: &Tensor,
        cond: &Tensor,
        noise: &Tensor,
        steps: usize,
        cfg: f64,
        cancel: &impl Fn() -> bool,
    ) -> Result<Tensor> {
        let mut x = noise.clone();
        let paired_mu = Tensor::cat(&[mu.clone(), mu.zeros_like()?], 0)?;
        self.prepare_schedule(steps, mu, cancel)?;
        let schedule = self.schedule.as_ref().expect("prepared schedule");
        let grid = &schedule.grid;
        // Keep the same batch-two projection arithmetic as the numerical oracle.
        let paired_cond = self
            .dit_cond
            .forward(&Tensor::cat(&[cond, cond], 0)?.transpose(1, 2)?)?;
        let skip = ((steps + 1) as f32 * 0.04).floor().max(1.0) as usize;
        for i in 0..steps {
            if cancel() {
                candle_core::bail!("generation cancelled");
            }
            if i < skip {
                continue;
            }
            let sequence = self.prepared_tokens(
                &Tensor::cat(&[&x, &x], 0)?,
                &paired_mu,
                &paired_cond,
                &schedule.times[i],
            )?;
            let v = self
                .dit_out
                .forward(&self.dit.forward(&sequence, None, cancel)?.narrow(1, 7, 4)?)?
                .transpose(1, 2)?
                .to_dtype(DType::F32)?;
            let pos = v.narrow(0, 0, 1)?;
            let neg = v.narrow(0, 1, 1)?;
            let dot = (&pos * &neg)?.sum_keepdim((1, 2))?;
            let sq = (neg.sqr()?.sum_keepdim((1, 2))? + 1e-8)?;
            let scaled = neg.broadcast_mul(&(dot / sq)?)?;
            let derivative = (&scaled + ((&pos - &scaled)? * cfg)?)?;
            x = (x - (derivative * (grid[i] - grid[i + 1]) as f64)?)?;
        }
        x.to_dtype(mu.dtype())
    }
}

fn round_ties_even(x: &Tensor) -> Result<Tensor> {
    let lower = x.floor()?;
    let fraction = (x - &lower)?;
    let parity = (&lower - ((&lower / 2.0)?.floor()? * 2.0)?)?;
    let tie = parity.eq(0.0)?.where_cond(&lower, &(&lower + 1.0)?)?;
    fraction.eq(0.5)?.where_cond(&tie, &x.round()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fsq_uses_torch_ties_even_for_both_signs() {
        let x = Tensor::new(
            &[-2.5f32, -1.5, -0.5, 0.5, 1.5, 2.5, 2.6],
            &candle_core::Device::Cpu,
        )
        .unwrap();
        assert_eq!(
            round_ties_even(&x).unwrap().to_vec1::<f32>().unwrap(),
            vec![-2.0, -2.0, 0.0, 0.0, 2.0, 2.0, 3.0]
        );
    }
}
