//! # Qwen3-TTS
//!
//! Pure Rust inference for [Qwen3-TTS](https://github.com/QwenLM/Qwen3-TTS),
//! a high-quality text-to-speech model from Alibaba.
//!
//! ## Features
//!
//! - **CPU inference**
//! - **CUDA** support for NVIDIA GPU acceleration
//! - **Metal** support for Apple Silicon
//! - **Streaming-friendly** architecture with incremental token generation
//! - **Voice cloning** via ECAPA-TDNN speaker encoder (Base models)
//! - **Auto-detection** of model variant from `config.json`
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use qwen3_tts::{Qwen3TTS, SynthesisOptions, auto_device};
//!
//! // Load model — variant auto-detected from config.json
//! let device = auto_device()?;
//! let model = Qwen3TTS::from_pretrained("path/to/model", device)?;
//!
//! // Synthesize speech with default settings
//! let audio = model.synthesize("Hello, world!", None)?;
//! audio.save("output.wav")?;
//!
//! // Or with custom options
//! let options = SynthesisOptions {
//!     temperature: 0.8,
//!     top_k: 30,
//!     ..Default::default()
//! };
//! let audio = model.synthesize("Custom settings!", Some(options))?;
//! ```
//!
//! ## Architecture
//!
//! The TTS pipeline consists of three stages:
//!
//! 1. **TalkerModel**: Transformer that generates semantic tokens from text
//!    autoregressively. Uses dual embeddings (text + codec) with MRoPE
//!    (multimodal rotary position encoding) across all variants.
//!
//! 2. **CodePredictor**: For each semantic token, generates 15 acoustic
//!    tokens using a 5-layer autoregressive decoder. The code predictor
//!    always has `hidden_size=1024` regardless of the talker size; 1.7B
//!    models use a `small_to_mtp_projection` layer to bridge the gap.
//!
//! 3. **Decoder12Hz**: Converts the 16-codebook codec tokens to audio
//!    waveform at 24kHz. Uses ConvNeXt blocks and transposed convolutions
//!    for upsampling. Shared across all model variants.
//!
//! ## Model Variants
//!
//! Five official variants exist in two size classes:
//!
//! | Variant | Size | Talker hidden | Speaker conditioning | HuggingFace ID |
//! |---------|------|---------------|---------------------|----------------|
//! | 0.6B Base | 1.8 GB | 1024 | Voice cloning (ECAPA-TDNN) | `Qwen/Qwen3-TTS-12Hz-0.6B-Base` |
//! | 0.6B CustomVoice | 1.8 GB | 1024 | 9 preset speakers | `Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice` |
//! | 1.7B Base | 3.9 GB | 2048 | Voice cloning (ECAPA-TDNN) | `Qwen/Qwen3-TTS-12Hz-1.7B-Base` |
//! | 1.7B CustomVoice | 3.9 GB | 2048 | 9 preset speakers | `Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice` |
//! | 1.7B VoiceDesign | 3.8 GB | 2048 | Text-described voices | `Qwen/Qwen3-TTS-12Hz-1.7B-VoiceDesign` |
//!
//! **Base**: Includes a speaker encoder for voice cloning from reference audio.
//! Supports x_vector_only (speaker embedding) and ICL (in-context learning
//! with reference audio + text) modes.
//!
//! **CustomVoice**: 9 preset speakers (Serena, Vivian, Ryan, Aiden, etc.) with
//! no speaker encoder. Uses discrete speaker token IDs for voice selection.
//!
//! **VoiceDesign**: Creates novel voices from text descriptions (e.g.,
//! "a deep male voice"). No speaker encoder or preset speakers.
//!
//! All variants share the same speech tokenizer and decoder weights. The
//! code predictor architecture is identical (1024 hidden, 5 layers, 16 heads)
//! across all variants.
//!
//! ## Sample Rate
//!
//! Output audio is always 24kHz mono. Use [`audio::resample()`] if you need
//! a different sample rate.

pub mod audio;
pub mod device;
pub mod generation;
pub mod models;
pub mod profiling;
mod synthesis;
pub mod tokenizer;

pub use audio::AudioBuffer;
pub use candle_core::Device;
pub use device::{auto_device, device_info, parse_device};
pub use generation::SamplingContext;
pub use models::config::Qwen3TTSConfig;
pub use models::talker::{Language, Speaker, codec_tokens, special_tokens, tts_tokens};
pub use models::{
    CodePredictor, CodePredictorConfig, ModelType, ParsedModelConfig, SpeakerEncoderConfig,
    TalkerConfig, TalkerModel,
};
pub use synthesis::{
    CODEC_EOS_TOKEN_ID, FrameCodes, Qwen3TTS, SAMPLES_PER_FRAME, StreamingSession,
    SynthesisOptions, SynthesisTiming, VoiceClonePrompt, codes_to_tensor, compute_dtype_for_device,
    sync_device,
};
