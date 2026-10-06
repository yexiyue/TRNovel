use tts_protocol::Event;
pub const REVISION: &str = "85e237c12c027371202489a0ec509ded67b5e4b5";
pub async fn prepare(
    directory: &std::path::Path,
    progress: tokio::sync::mpsc::Sender<Event>,
) -> anyhow::Result<()> {
    crate::resources::prepare(
        directory,
        "qwen",
        serde_json::from_str(include_str!("resources.json"))?,
        progress,
    )
    .await
}
