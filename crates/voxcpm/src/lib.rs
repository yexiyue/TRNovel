//! VoxCPM2 using the workspace Candle runtime.
//!
//! The production adapter uses the existing GGUF path. Original Safetensors
//! weights are available for explicit precision and performance qualification.
mod acoustic;
mod codec;
mod model;
mod timing;
mod transformer;
#[cfg(test)]
mod validation;
mod weights;
pub use model::{Model, Options, Outcome, Reference, SAMPLE_RATE};
pub use timing::Timings;
pub mod tokenizer;
