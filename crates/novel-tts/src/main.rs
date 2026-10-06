mod cli;
mod protocol;
mod resources;
mod runtime;

use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    version,
    about = "Read a UTF-8 file aloud, or serve JSON Lines on stdin/stdout"
)]
struct Args {
    /// File to read. Interactive controls: space=pause/resume, s=stop, q=exit.
    #[arg(required_unless_present = "protocol", conflicts_with = "protocol")]
    file: Option<PathBuf>,
    /// Machine mode; never takes over the terminal.
    #[arg(long)]
    protocol: bool,
    /// Explicitly restart this file from its beginning, ignoring a stored checkpoint.
    #[arg(long, conflicts_with = "protocol")]
    restart: bool,
    /// Override the legacy listening configuration path.
    #[arg(long)]
    config: Option<PathBuf>,
    /// Override the model directory (does not copy or delete existing models).
    #[arg(long)]
    model_dir: Option<PathBuf>,
    /// Override the independent listening checkpoint directory.
    #[arg(long)]
    checkpoint_dir: Option<PathBuf>,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let config = match args.config {
        Some(path) => tts_core::config::ConfigStore::new(path),
        None => tts_core::config::ConfigStore::user_default()?,
    };
    let checkpoints = match args.checkpoint_dir {
        Some(path) => tts_core::checkpoint::CheckpointStore::new(path),
        None => tts_core::checkpoint::CheckpointStore::user_default()?,
    };
    let resources = resources::Resources::new(args.model_dir);
    tokio::task::LocalSet::new()
        .run_until(async move {
            if args.protocol {
                protocol::run(config, checkpoints, resources).await
            } else {
                cli::run(
                    args.file.expect("clap requires a file"),
                    config,
                    checkpoints,
                    resources,
                    !args.restart,
                )
                .await
            }
        })
        .await
}
