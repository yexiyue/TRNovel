use std::path::Path;
use tokio::sync::mpsc;
use tts_protocol::Event;
pub const REVISION: &str = "169f64d8b98bbaab1761e4ca3a83e6af653456cc";
pub async fn prepare(directory: &Path, progress: mpsc::Sender<Event>) -> anyhow::Result<()> {
    crate::resources::prepare(
        directory,
        "voxcpm/2b-q8_0",
        serde_json::from_str(include_str!("resources.json"))?,
        progress,
    )
    .await
}
