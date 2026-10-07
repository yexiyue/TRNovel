use std::path::Path;
use tokio::sync::mpsc;
use tts_protocol::Event;
pub const REVISION: &str = "169f64d8b98bbaab1761e4ca3a83e6af653456cc";
pub async fn prepare(directory: &Path, progress: mpsc::Sender<Event>) -> anyhow::Result<()> {
    prepare_model(directory, super::models::Model::Q8, progress).await
}
pub async fn prepare_model(
    directory: &Path,
    model: super::models::Model,
    progress: mpsc::Sender<Event>,
) -> anyhow::Result<()> {
    crate::resources::prepare(
        directory,
        &format!("voxcpm/{}", model.id()),
        serde_json::from_str(model.manifest())?,
        progress,
    )
    .await
}
