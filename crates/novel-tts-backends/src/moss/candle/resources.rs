use super::Mode;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;
use tts_protocol::Event;
pub const CODEC_REVISION: &str = "3cd226ba2947efa357ef453bcad111b6eafba782";
pub const DESIGN_REVISION: &str = "97521ec2b6f3ec5026ac1f5751f8fc302d82c2d4";
pub fn codec_directory(root: &Path) -> PathBuf {
    root.join("moss/models/codec").join(CODEC_REVISION)
}
pub fn design_directory(root: &Path) -> PathBuf {
    root.join("moss/models/voice-design-1.7b")
        .join(DESIGN_REVISION)
}
#[derive(Deserialize)]
struct Manifest {
    resources: Vec<crate::resources::Resource>,
}
async fn manifest(
    directory: &Path,
    id: &str,
    json: &str,
    progress: mpsc::Sender<Event>,
) -> anyhow::Result<()> {
    crate::resources::prepare(
        directory,
        &format!("moss/{id}"),
        serde_json::from_str::<Manifest>(json)?.resources,
        progress,
    )
    .await
}
pub async fn prepare(root: &Path, mode: Mode, progress: mpsc::Sender<Event>) -> anyhow::Result<()> {
    let json = match mode {
        Mode::Local => include_str!("../../../../moss-tts/assets/local-1.7b.json"),
        Mode::Realtime => include_str!("../../../../moss-tts/assets/realtime-1.7b.json"),
    };
    manifest(&mode.directory(root), mode.id(), json, progress.clone()).await?;
    codec(root, progress).await
}
async fn codec(root: &Path, progress: mpsc::Sender<Event>) -> anyhow::Result<()> {
    manifest(
        &codec_directory(root),
        "codec",
        include_str!("../../../../moss-tts/assets/codec.json"),
        progress,
    )
    .await
}
pub async fn prepare_design(root: &Path, progress: mpsc::Sender<Event>) -> anyhow::Result<()> {
    manifest(
        &design_directory(root),
        "voice-design-1.7b",
        include_str!("../../../../moss-tts/assets/voice-design-1.7b.json"),
        progress.clone(),
    )
    .await?;
    codec(root, progress).await
}
