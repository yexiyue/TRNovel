//! Residual normalization composed from Candle's device-specific operators.
use anyhow::Result;
use candle_core::{Module, Tensor};
use candle_nn::{RmsNorm, VarBuilder, rms_norm};

pub struct ResidualRmsNorm {
    inner: RmsNorm,
}
impl ResidualRmsNorm {
    pub fn load(hidden_size: usize, eps: f64, vb: VarBuilder) -> Result<Self> {
        Ok(Self {
            inner: rms_norm(hidden_size, eps, vb)?,
        })
    }
    pub fn forward(&self, x: &Tensor) -> Result<Tensor> {
        Ok(self.inner.forward(x)?)
    }
    pub fn forward_residual(&self, x: &Tensor, residual: &Tensor) -> Result<(Tensor, Tensor)> {
        let sum = (x + residual)?;
        Ok((self.inner.forward(&sum)?, sum))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::{DType, Device};
    use candle_nn::VarMap;

    fn create_fused_rms_norm(hidden_size: usize, eps: f64, device: &Device) -> ResidualRmsNorm {
        let varmap = VarMap::new();
        let vb = VarBuilder::from_varmap(&varmap, DType::F32, device);
        ResidualRmsNorm::load(hidden_size, eps, vb).unwrap()
    }

    #[test]
    fn test_fused_rmsnorm_forward_matches_standard() {
        let device = Device::Cpu;
        let hidden = 64;
        let eps = 1e-6;

        let norm = create_fused_rms_norm(hidden, eps, &device);

        let x = Tensor::randn(0.0f32, 1.0, (2, 10, hidden), &device).unwrap();
        let out_standard = norm.forward(&x).unwrap();
        let out_inner = norm.inner.forward(&x).unwrap();

        let diff = (&out_standard - &out_inner)
            .unwrap()
            .abs()
            .unwrap()
            .max(0)
            .unwrap()
            .max(0)
            .unwrap()
            .max(0)
            .unwrap()
            .to_scalar::<f32>()
            .unwrap();
        assert!(diff < 1e-5, "forward mismatch: max diff = {diff}");
    }

    #[test]
    fn test_fused_residual_rmsnorm_sequential() {
        let device = Device::Cpu;
        let hidden = 64;
        let eps = 1e-6;

        let norm = create_fused_rms_norm(hidden, eps, &device);

        let x = Tensor::randn(0.0f32, 1.0, (2, 10, hidden), &device).unwrap();
        let residual = Tensor::randn(0.0f32, 1.0, (2, 10, hidden), &device).unwrap();

        let (normed, sum) = norm.forward_residual(&x, &residual).unwrap();

        // Verify sum = x + residual
        let expected_sum = (&x + &residual).unwrap();
        let sum_diff = (&sum - &expected_sum)
            .unwrap()
            .abs()
            .unwrap()
            .max(0)
            .unwrap()
            .max(0)
            .unwrap()
            .max(0)
            .unwrap()
            .to_scalar::<f32>()
            .unwrap();
        assert!(sum_diff < 1e-6, "sum mismatch: {sum_diff}");

        // Verify normed = rms_norm(sum)
        let expected_normed = norm.inner.forward(&expected_sum).unwrap();
        let norm_diff = (&normed - &expected_normed)
            .unwrap()
            .abs()
            .unwrap()
            .max(0)
            .unwrap()
            .max(0)
            .unwrap()
            .max(0)
            .unwrap()
            .to_scalar::<f32>()
            .unwrap();
        assert!(norm_diff < 1e-5, "norm mismatch: {norm_diff}");
    }

    #[test]
    fn test_fused_rmsnorm_shapes() {
        let device = Device::Cpu;
        let hidden = 64;
        let norm = create_fused_rms_norm(hidden, 1e-6, &device);

        let x = Tensor::randn(0.0f32, 1.0, (1, 5, hidden), &device).unwrap();
        let residual = Tensor::randn(0.0f32, 1.0, (1, 5, hidden), &device).unwrap();

        let (normed, sum) = norm.forward_residual(&x, &residual).unwrap();
        assert_eq!(normed.dims(), &[1, 5, hidden]);
        assert_eq!(sum.dims(), &[1, 5, hidden]);
    }
}
