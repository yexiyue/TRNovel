//! Speech tokenizer encoder (12Hz Mimi-based)
//!
//! Encodes raw audio into 12Hz, 16-codebook discrete codec codes for ICL
//! voice cloning.
//!
//! The Qwen3-TTS speech tokenizer uses a standard HuggingFace Mimi model
//! for encoding. We construct only the encoder path (SEANet encoder →
//! transformer → downsample → quantizer) from candle's public Mimi
//! components, skipping the decoder entirely.
//!
//! ## Weight key mapping
//!
//! The HF model wraps all encoder components under an `encoder.` prefix:
//! - `encoder.encoder.layers.*`       → SEANet convolutional encoder
//! - `encoder.encoder_transformer.*`  → Streaming transformer
//! - `encoder.downsample.*`           → 25Hz→12.5Hz frame rate conversion
//! - `encoder.quantizer.*`            → Split residual vector quantizer
//!
//! Stripping this prefix yields keys compatible with candle's Mimi format.

use anyhow::Result;
use candle_core::{DType, Device, Module, StreamingModule, Tensor};
use candle_transformers::models::mimi;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;

use crate::audio::AudioBuffer;

/// Mimi-based encoder producing 12Hz, 16-codebook codes from raw audio.
///
/// Constructs the encoder-only path from candle's Mimi components (no
/// decoder). Used by ICL voice cloning to tokenize reference audio.
pub struct Encoder12Hz {
    encoder: mimi::seanet::SeaNetEncoder,
    // RefCell because ProjectedTransformer::forward/reset_state need &mut self
    encoder_transformer: RefCell<mimi::transformer::ProjectedTransformer>,
    downsample: mimi::conv::ConvDownsample1d,
    quantizer: mimi::quantization::SplitResidualVectorQuantizer,
    device: Device,
}

impl Encoder12Hz {
    /// Load from a safetensors file (e.g., `speech_tokenizer/model.safetensors`).
    pub fn from_safetensors(path: &Path, device: &Device) -> Result<Self> {
        let raw_tensors: HashMap<String, Tensor> = candle_core::safetensors::load(path, device)?;
        Self::from_weights(&raw_tensors, device)
    }

    /// Load from pre-loaded weight tensors.
    ///
    /// Expects HF-format keys with `encoder.*` prefix. Strips the prefix
    /// and constructs encoder-only components.
    pub fn from_weights(weights: &HashMap<String, Tensor>, device: &Device) -> Result<Self> {
        // The HF Qwen3-TTS speech tokenizer wraps all encoder components
        // under an "encoder." prefix. Strip it to match candle's Mimi format:
        //   encoder.encoder.layers.0.conv.weight      → encoder.layers.0.conv.weight
        //   encoder.encoder_transformer.layers.0.*     → encoder_transformer.layers.0.*
        //   encoder.quantizer.*                        → quantizer.*
        //   encoder.downsample.conv.weight             → downsample.conv.weight
        let encoder_weights: HashMap<String, Tensor> = weights
            .iter()
            .filter_map(|(k, v)| {
                k.strip_prefix("encoder.")
                    .map(|stripped| (stripped.to_string(), v.clone()))
            })
            .collect();

        if encoder_weights.is_empty() {
            anyhow::bail!("No encoder keys found (expected keys starting with 'encoder.')");
        }

        let cfg = mimi::Config::v0_1(Some(16));
        let vb = candle_nn::VarBuilder::from_tensors(encoder_weights, DType::F32, device);

        let encoder = mimi::seanet::SeaNetEncoder::new(&cfg.seanet, vb.pp("encoder"))?;
        let encoder_transformer = mimi::transformer::ProjectedTransformer::new(
            cfg.seanet.dimension,
            &[cfg.seanet.dimension],
            &cfg.transformer,
            vb.pp("encoder_transformer"),
        )?;

        // SEANet produces 25Hz (24000 / (8*6*5*4) = 25), downsample to 12.5Hz
        let encoder_frame_rate =
            cfg.sample_rate / cfg.seanet.ratios.iter().product::<usize>() as f64;
        let downsample_stride = (encoder_frame_rate / cfg.frame_rate) as usize;
        let downsample = mimi::conv::ConvDownsample1d::new(
            downsample_stride,
            cfg.seanet.dimension,
            /* causal */ true,
            /* learnt */ true,
            vb.pp("downsample"),
        )?;

        let quantizer = mimi::quantization::SplitResidualVectorQuantizer::new(
            cfg.quantizer_dim,
            Some(cfg.seanet.dimension),
            Some(cfg.seanet.dimension),
            cfg.quantizer_n_q,
            cfg.quantizer_bins,
            vb.pp("quantizer"),
        )?;

        Ok(Self {
            encoder,
            encoder_transformer: RefCell::new(encoder_transformer),
            downsample,
            quantizer,
            device: device.clone(),
        })
    }

    /// Encode audio to discrete codec codes.
    ///
    /// Input: `AudioBuffer` at 24kHz.
    /// Output: `Tensor` of shape `[T_frames, 16]` containing discrete codes (u32).
    pub fn encode(&self, audio: &AudioBuffer) -> Result<Tensor> {
        let samples = &audio.samples;

        // Mimi expects [batch, channels, samples] = [1, 1, N]
        let input = Tensor::from_vec(samples.to_vec(), (1, 1, samples.len()), &self.device)?;
        let input = input.to_dtype(DType::F32)?;

        // SEANet encoder: [1, 1, N] → [1, 512, T_25hz]
        let xs = self.encoder.forward(&input)?;

        // Transformer: [1, 512, T_25hz] → [1, T_25hz, 512] (conv_layout transposes internally)
        let mut transformer = self.encoder_transformer.borrow_mut();
        transformer.reset_state();
        let xs = transformer.forward(&xs)?;
        let xs = &xs[0];

        // Downsample 25Hz → 12.5Hz
        let xs = xs.apply(&self.downsample)?;

        // Quantize: [1, 512, T_12hz] → [1, 16, T_12hz]
        let codes = self.quantizer.encode(&xs)?;

        // Transpose to [T_frames, 16]
        let codes = codes.squeeze(0)?.transpose(0, 1)?;
        Ok(codes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_encoder_prefix() {
        let mut weights = HashMap::new();
        let dummy = Tensor::zeros((1,), DType::F32, &Device::Cpu).unwrap();
        weights.insert(
            "encoder.encoder.layers.0.conv.weight".to_string(),
            dummy.clone(),
        );
        weights.insert(
            "encoder.encoder_transformer.layers.0.self_attn.q_proj.weight".to_string(),
            dummy.clone(),
        );
        weights.insert(
            "encoder.quantizer.semantic_residual_vector_quantizer.layers.0.codebook.embed_sum"
                .to_string(),
            dummy.clone(),
        );
        weights.insert("encoder.downsample.conv.weight".to_string(), dummy.clone());
        // Decoder keys should be excluded
        weights.insert("decoder.decoder.0.conv.weight".to_string(), dummy);

        let stripped: HashMap<String, Tensor> = weights
            .iter()
            .filter_map(|(k, v)| {
                k.strip_prefix("encoder.")
                    .map(|s| (s.to_string(), v.clone()))
            })
            .collect();

        assert!(stripped.contains_key("encoder.layers.0.conv.weight"));
        assert!(stripped.contains_key("encoder_transformer.layers.0.self_attn.q_proj.weight"));
        assert!(stripped.contains_key(
            "quantizer.semantic_residual_vector_quantizer.layers.0.codebook.embed_sum"
        ));
        assert!(stripped.contains_key("downsample.conv.weight"));
        assert!(!stripped.contains_key("decoder.decoder.0.conv.weight"));
        assert_eq!(stripped.len(), 4);
    }
}
