//! Audio codec for Qwen3-TTS
//!
//! This module implements the audio encoder/decoder that converts
//! between raw audio waveforms and discrete codec tokens.
//!
//! Two codec variants are supported:
//! - 12Hz (Mimi-based): Higher quality, newer architecture
//! - 25Hz (BigVGAN-based): Faster, more established

pub mod causal_conv;
pub mod causal_trans_conv;
pub mod convnext_block;
pub mod decoder;
pub mod decoder_12hz;
pub mod decoder_block;
pub mod encoder_12hz;
mod quantizer;
pub mod snake_beta;

pub use causal_conv::CausalConv1d;
pub use causal_trans_conv::CausalTransConv1d;
pub use convnext_block::ConvNeXtBlock;
pub use decoder::{CodecDecoder, DecoderConfig};
pub use decoder_12hz::{Decoder12Hz, Decoder12HzConfig};
pub use decoder_block::{DecoderBlock, ResidualUnit};
pub use encoder_12hz::Encoder12Hz;
pub use quantizer::{ResidualVectorQuantizer, VectorQuantizer};
pub use snake_beta::SnakeBeta;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decoder_12hz_config_default() {
        let config = Decoder12HzConfig::default();
        assert_eq!(config.codebook_dim, 512);
        assert_eq!(config.num_quantizers, 16);
        assert_eq!(config.upsample_rates, vec![8, 5, 4, 3]);
    }
}
