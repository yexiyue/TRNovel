//! TTS-specific generation logic
//!
//! Token suppression during sampling to prevent the model from generating
//! tokens in the reserved control range.

use anyhow::Result;
use candle_core::{DType, Device, Tensor};

/// Pre-computed boolean mask for token suppression.
///
/// Build once with [`build_suppression_mask`], then apply cheaply each frame
/// with [`apply_token_suppression_with_mask`].
pub struct SuppressionMask {
    /// Boolean mask: true at positions to suppress. Shape [1, vocab].
    mask: Tensor,
}

/// Build a reusable suppression mask for the given vocab/EOS config.
///
/// The mask is a [1, vocab] boolean tensor that can be broadcast to any batch size.
pub fn build_suppression_mask(
    vocab_size: usize,
    eos_token_id: u32,
    device: &Device,
) -> Result<SuppressionMask> {
    let suppress_start = vocab_size - 1024;
    let mut mask_data = vec![0u8; vocab_size];
    for (v, val) in mask_data
        .iter_mut()
        .enumerate()
        .skip(suppress_start)
        .take(1024)
    {
        if v as u32 != eos_token_id {
            *val = 1;
        }
    }
    let mask = Tensor::new(mask_data.as_slice(), device)?.unsqueeze(0)?; // [1, vocab]
    // Convert to boolean by comparing > 0
    let zeros = Tensor::zeros((1, vocab_size), DType::U8, device)?;
    let mask = mask.gt(&zeros)?;
    Ok(SuppressionMask { mask })
}

/// Apply a pre-built suppression mask to logits (cheap per-frame operation).
pub fn apply_token_suppression_with_mask(
    logits: &Tensor,
    suppression: &SuppressionMask,
) -> Result<Tensor> {
    let mask = suppression.mask.broadcast_as(logits.shape())?;
    let neg_inf =
        Tensor::new(&[f32::NEG_INFINITY], logits.device())?.broadcast_as(logits.shape())?;
    Ok(mask.where_cond(&neg_inf, logits)?)
}

/// Apply token suppression to logits (builds mask each call — use the mask
/// variant for hot loops).
///
/// Masks out tokens in range `[vocab_size - 1024, vocab_size)` except for the
/// EOS token, which is preserved.
pub fn apply_token_suppression(
    logits: &Tensor,
    vocab_size: usize,
    eos_token_id: u32,
) -> Result<Tensor> {
    let suppression = build_suppression_mask(vocab_size, eos_token_id, logits.device())?;
    apply_token_suppression_with_mask(logits, &suppression)
}

#[cfg(test)]
mod tests {
    use super::*;
    use candle_core::Device;

    #[test]
    fn test_suppression_masks_control_tokens() {
        let device = Device::Cpu;
        let vocab_size = 3072;
        let eos_id = 2150u32;
        // All logits at 1.0
        let logits = Tensor::ones((1, vocab_size), candle_core::DType::F32, &device).unwrap();
        let result = apply_token_suppression(&logits, vocab_size, eos_id).unwrap();
        let vals: Vec<f32> = result.flatten_all().unwrap().to_vec1().unwrap();

        // Non-control tokens should be unchanged
        assert!((vals[0] - 1.0).abs() < 1e-6);
        assert!((vals[2047] - 1.0).abs() < 1e-6);

        // Control tokens (except EOS) should be -inf
        assert!(vals[2048].is_infinite() && vals[2048] < 0.0); // suppress_start = 3072 - 1024 = 2048
        assert!(vals[2149].is_infinite() && vals[2149] < 0.0); // 2149 != 2150

        // EOS should be preserved
        assert!((vals[2150] - 1.0).abs() < 1e-6);

        // Other control tokens suppressed
        assert!(vals[2151].is_infinite() && vals[2151] < 0.0);
        assert!(vals[3071].is_infinite() && vals[3071] < 0.0);
    }

    #[test]
    fn test_suppression_batch() {
        let device = Device::Cpu;
        let vocab_size = 3072;
        let eos_id = 2150u32;
        let logits = Tensor::ones((2, vocab_size), candle_core::DType::F32, &device).unwrap();
        let result = apply_token_suppression(&logits, vocab_size, eos_id).unwrap();
        let vals: Vec<f32> = result.flatten_all().unwrap().to_vec1().unwrap();

        // Both batches should have suppression
        assert!(vals[2048].is_infinite()); // batch 0
        assert!(vals[vocab_size + 2048].is_infinite()); // batch 1

        // EOS preserved in both
        assert!((vals[2150] - 1.0).abs() < 1e-6);
        assert!((vals[vocab_size + 2150] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn test_prebuilt_mask_matches_inline() {
        let device = Device::Cpu;
        let vocab_size = 3072;
        let eos_id = 2150u32;
        let logits = Tensor::ones((1, vocab_size), candle_core::DType::F32, &device).unwrap();

        // Inline path
        let result_inline = apply_token_suppression(&logits, vocab_size, eos_id).unwrap();

        // Pre-built mask path
        let mask = build_suppression_mask(vocab_size, eos_id, &device).unwrap();
        let result_prebuilt = apply_token_suppression_with_mask(&logits, &mask).unwrap();

        let a: Vec<f32> = result_inline.flatten_all().unwrap().to_vec1().unwrap();
        let b: Vec<f32> = result_prebuilt.flatten_all().unwrap().to_vec1().unwrap();
        for (i, (va, vb)) in a.iter().zip(b.iter()).enumerate() {
            assert!(
                (va - vb).abs() < 1e-9 || (va.is_infinite() && vb.is_infinite()),
                "Mismatch at index {}: inline={} prebuilt={}",
                i,
                va,
                vb,
            );
        }
    }

    #[test]
    fn test_prebuilt_mask_reusable_across_batches() {
        let device = Device::Cpu;
        let vocab_size = 3072;
        let eos_id = 2150u32;

        // Build mask once
        let mask = build_suppression_mask(vocab_size, eos_id, &device).unwrap();

        // Apply to batch=1
        let logits1 = Tensor::ones((1, vocab_size), candle_core::DType::F32, &device).unwrap();
        let r1 = apply_token_suppression_with_mask(&logits1, &mask).unwrap();
        assert!(r1.flatten_all().unwrap().to_vec1::<f32>().unwrap()[2048].is_infinite());

        // Apply to batch=3 (same mask reused)
        let logits3 = Tensor::ones((3, vocab_size), candle_core::DType::F32, &device).unwrap();
        let r3 = apply_token_suppression_with_mask(&logits3, &mask).unwrap();
        let vals: Vec<f32> = r3.flatten_all().unwrap().to_vec1().unwrap();
        // Check suppression in all 3 batches
        for batch in 0..3 {
            assert!(vals[batch * vocab_size + 2048].is_infinite());
            assert!((vals[batch * vocab_size + 2150] - 1.0).abs() < 1e-6); // EOS preserved
        }
    }
}
