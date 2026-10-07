#![deny(unsafe_op_in_unsafe_fn)]

pub mod artifacts;
pub mod audio_input;
pub mod audio_tokenizer;
mod codec_layers;
pub mod contracts;
pub mod error;
pub mod frontend;
pub mod model_source;
mod paths;
pub mod pipeline;
pub mod postprocess;
pub mod reference_prompt;
pub mod runtime;
pub mod stage0_loop;
pub mod stage0_model;
pub mod stage0_qwen3;
pub mod stage0_qwen3_quantized;
pub mod stage1_decoder;
pub mod stage1_model;

pub use artifacts::ModelWeightDType;
pub use contracts::{BoolTensor2, BoolTensor4, GeneratedAudioResult, GenerationUsage};
pub use error::{OmniVoiceError, Result};
pub use runtime::{DTypeSpec, DeviceSpec, RuntimeOptions};
