use tts_protocol::Event;
pub const REVISION: &str = "85e237c12c027371202489a0ec509ded67b5e4b5";
pub async fn prepare(
    directory: &std::path::Path,
    progress: tokio::sync::mpsc::Sender<Event>,
) -> anyhow::Result<()> {
    prepare_model(directory, super::models::Model::Custom06, progress).await
}
pub async fn prepare_model(
    directory: &std::path::Path,
    model: super::models::Model,
    progress: tokio::sync::mpsc::Sender<Event>,
) -> anyhow::Result<()> {
    crate::resources::prepare(
        directory,
        &format!("qwen/{}", model.id()),
        serde_json::from_str(model.manifest())?,
        progress,
    )
    .await
}
