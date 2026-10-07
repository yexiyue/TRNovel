//! Target-scoped Cargo feature activation for one workspace Candle version.
//!
//! Candle's GPU features are not target-gated upstream. Routing them through
//! optional platform dependencies prevents `--all-features` on Windows from
//! building Objective-C, and prevents macOS builds from invoking nvcc.
//! Transformers' CUDA feature activates both core and NN; NN's Metal feature
//! activates core and Metal kernels. This crate contains no inference code.
