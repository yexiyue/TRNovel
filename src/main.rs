#[tokio::main]
async fn main() -> anyhow::Result<()> {
    novel_tui::run().await
}
