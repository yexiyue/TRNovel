//! Immutable MOSS model manifest.
use std::path::Path;
use tts_protocol::Event;
pub const REVISION: &str =
    "f52645cb467506d8e18e746ddd59482685b74e58/ceff0d0749bfb3fa2d61149794ec6feef0d1e1ae";
pub async fn prepare(
    directory: &Path,
    progress: tokio::sync::mpsc::Sender<Event>,
) -> anyhow::Result<()> {
    let resources = serde_json::from_str(include_str!("assets/resources.json"))?;
    crate::resources::prepare(directory, "moss", resources, progress).await
}
