//! Pinned aligner assets, verified before model initialization.
use std::path::Path;
use tts_protocol::Event;
pub const REVISION: &str = "261c9ed100c1b18a4a1fbc488e05625dc9a4ae5c";
const CPU_ASSETS: &[(&str, u64, &str)] = &[
    (
        "model_q4.onnx",
        1048397349,
        "59b528896d70b34e57838e160d16d5f7cfc02d86c7c6ad46cdc57c25c15497b7",
    ),
    (
        "tokenizer.json",
        11429733,
        "e95e4127f5ea82f89695f00bb3c143eb7edd411e6096e27ff56319397f481e77",
    ),
];
const GPU_ASSETS: &[(&str, u64, &str)] = &[
    (
        "model.onnx",
        1917718,
        "7b2bff8b7a8df4120b450673d65915488e5fd43e107f87f3816d9e2c830a9e8b",
    ),
    (
        "model.onnx_data",
        3670969192,
        "429a19b51f5f8a2504b5e573de87c8f74e2ad0be68e0ff7ba1e645663507bcdc",
    ),
];
pub async fn prepare(
    directory: &Path,
    progress: tokio::sync::mpsc::Sender<Event>,
) -> anyhow::Result<()> {
    prepare_on(directory, progress, tts_protocol::Device::Cpu).await
}
pub async fn prepare_on(
    directory: &Path,
    progress: tokio::sync::mpsc::Sender<Event>,
    device: tts_protocol::Device,
) -> anyhow::Result<()> {
    let resources: Vec<_> = if device == tts_protocol::Device::Cpu {
        CPU_ASSETS.to_vec()
    } else {
        GPU_ASSETS
            .iter()
            .copied()
            .chain(
                CPU_ASSETS
                    .iter()
                    .copied()
                    .filter(|(name, _, _)| *name == "tokenizer.json"),
            )
            .collect()
    };
    let resources = resources.into_iter().map(|(name, size, checksum)| {
        let remote = if name.starts_with("model") { format!("onnx/{name}") } else { name.into() };
        crate::resources::Resource {
            path: name.into(), size, sha256: checksum.into(),
            url: format!("https://huggingface.co/valoomba/Qwen3-ForcedAligner-0.6B-ONNX/resolve/{REVISION}/{remote}"),
        }
    }).collect();
    crate::resources::prepare(directory, "alignment", resources, progress).await
}
