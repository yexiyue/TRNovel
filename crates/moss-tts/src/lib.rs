//! Native MOSS computation on the workspace Candle runtime.
//!
//! Model scheduling and the audio codec are independent of downloads, playback
//! and configuration. Candidates remain outside the worker catalog until verified.
pub mod codec;
pub mod config;
pub mod delay;
pub mod local;
pub mod prompt;
pub mod realtime;
mod sampling;
mod transformer;

/// Different scheduling architectures share the same tensor runtime and codec.
pub enum Model {
    Local(local::LocalModel),
    Realtime(realtime::RealtimeModel),
    VoiceDesign(delay::DelayModel),
}
impl Model {
    pub fn load(
        mode: &str,
        directory: &Path,
        device: &Device,
        dtype: DType,
    ) -> anyhow::Result<Self> {
        Ok(match mode {
            "local-1.7b" => Self::Local(local::LocalModel::load(directory, device, dtype)?),
            "realtime-1.7b" => {
                Self::Realtime(realtime::RealtimeModel::load(directory, device, dtype)?)
            }
            "voice-design-1.7b" => {
                Self::VoiceDesign(delay::DelayModel::load(directory, device, dtype)?)
            }
            _ => anyhow::bail!("unknown MOSS mode {mode}"),
        })
    }
    pub fn codebooks(&self) -> usize {
        match self {
            Self::Local(model) => model.config.n_vq,
            Self::Realtime(_) => 16,
            Self::VoiceDesign(model) => model.config.n_vq,
        }
    }
    pub fn generate(
        &mut self,
        request: &Generation<'_>,
        cancelled: &impl Fn() -> bool,
        frame: impl FnMut(&[u32]) -> anyhow::Result<bool>,
    ) -> anyhow::Result<()> {
        match self {
            Self::Local(model) => model.generate(request, cancelled, frame),
            Self::Realtime(model) => model.generate(request, cancelled, frame),
            Self::VoiceDesign(model) => model.generate(request, cancelled, frame),
        }
    }
}

/// Input and generation budget shared by model schedulers.
pub struct Generation<'a> {
    pub text: &'a str,
    pub instruction: Option<&'a str>,
    pub reference: Option<&'a [Vec<u32>]>,
    pub max_frames: usize,
    pub seed: u64,
}
impl<'a> Generation<'a> {
    pub fn new(text: &'a str) -> Self {
        Self {
            text,
            instruction: None,
            reference: None,
            max_frames: 750,
            seed: 42,
        }
    }
}

use candle_core::{DType, Device};
use candle_nn::VarBuilder;
use std::path::{Path, PathBuf};

/// Open official safetensors shards. The mapped files outlive their tensors.
pub fn weights(
    directory: &Path,
    dtype: DType,
    device: &Device,
) -> anyhow::Result<VarBuilder<'static>> {
    let index = directory.join("model.safetensors.index.json");
    let files: Vec<PathBuf> = if index.exists() {
        let index: serde_json::Value = serde_json::from_slice(&std::fs::read(index)?)?;
        let mut names: Vec<_> = index["weight_map"]
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("invalid weight index"))?
            .values()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| anyhow::anyhow!("invalid shard name"))
            })
            .collect::<anyhow::Result<_>>()?;
        names.sort();
        names.dedup();
        names.into_iter().map(|name| directory.join(name)).collect()
    } else {
        vec![directory.join("model.safetensors")]
    };
    // SAFETY: model assets are immutable for the lifetime of the inference owner.
    Ok(unsafe { VarBuilder::from_mmaped_safetensors(&files, dtype, device)? })
}

pub(crate) fn check_cancel(cancelled: &impl Fn() -> bool) -> anyhow::Result<()> {
    anyhow::ensure!(!cancelled(), "MOSS inference cancelled");
    Ok(())
}
