use super::Variant;
use std::path::Path;
use tokio::sync::mpsc;
use tts_protocol::Event;
pub const REVISION: &str = "4ed45fb6e7e9527b780bef9e097a04bf13fe4e6b";
pub async fn prepare(
    directory: &Path,
    variant: Variant,
    progress: mpsc::Sender<Event>,
) -> anyhow::Result<()> {
    let manifest = match variant {
        Variant::Int8 => include_str!("distill-int8.json"),
        Variant::Fp32 => include_str!("distill-fp32.json"),
    };
    crate::resources::prepare(
        directory,
        &format!("zipvoice/{}", variant.id()),
        serde_json::from_str(manifest)?,
        progress,
    )
    .await
}
