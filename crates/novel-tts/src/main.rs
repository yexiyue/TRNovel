mod cli;
mod preparation;
mod protocol;
mod resources;
mod runtime;
mod voices;

use clap::Parser;
use std::path::PathBuf;

#[derive(clap::Subcommand)]
enum Commands {
    Voices {
        #[command(subcommand)]
        command: voices::VoiceCommand,
    },
}

#[derive(Parser)]
#[command(
    version,
    subcommand_negates_reqs = true,
    about = "Read a UTF-8 file aloud, or serve JSON Lines on stdin/stdout"
)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,
    #[arg(long)]
    backend: Option<String>,
    #[arg(long)]
    voice: Option<String>,
    #[arg(long, value_parser = parse_device)]
    tts_device: Option<tts_protocol::Device>,
    #[arg(long, value_parser = parse_device)]
    alignment_device: Option<tts_protocol::Device>,
    /// Enable or disable optional sentence alignment (disabled by default).
    #[arg(long, num_args = 0..=1, default_missing_value = "true", require_equals = true)]
    alignment: Option<bool>,
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
    let resources = resources::Resources::new(args.model_dir)?;
    if args.backend.is_some()
        || args.voice.is_some()
        || args.tts_device.is_some()
        || args.alignment_device.is_some()
        || args.alignment.is_some()
    {
        #[cfg(any(feature = "moss", feature = "kokoro"))]
        for (component, device) in [
            ("tts", args.tts_device),
            ("alignment", args.alignment_device),
        ]
        .into_iter()
        .filter_map(|(component, device)| device.map(|device| (component, device)))
        {
            preparation::validate_device(component, device)?;
        }
        let current = config.load()?;
        preparation::validate_alignment_enabled(
            args.alignment.unwrap_or(current.alignment_enabled),
        )?;
        let target = args.backend.as_deref().unwrap_or(&current.backend);
        let caps = resources.capabilities(target)?;
        let voice = args.voice.or_else(|| {
            args.backend
                .as_ref()
                .filter(|id| *id != &current.backend)
                .map(|_| caps.default_voice.clone())
        });
        config.update(
            &tts_protocol::ConfigPatch {
                expected_revision: current.revision,
                backend: args.backend,
                tts_device: args.tts_device,
                alignment_device: args.alignment_device,
                alignment_enabled: args.alignment,
                voice,
                ..Default::default()
            },
            &caps,
        )?;
    }
    tokio::task::LocalSet::new()
        .run_until(async move {
            if let Some(Commands::Voices { command }) = args.command {
                voices::run(command, resources).await
            } else if args.protocol {
                protocol::run(config, checkpoints, resources).await
            } else {
                cli::run(
                    args.file.ok_or_else(|| {
                        anyhow::anyhow!(
                            "provide a UTF-8 file, --protocol, or voices command; see --help"
                        )
                    })?,
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

fn parse_device(value: &str) -> Result<tts_protocol::Device, String> {
    match value {
        "auto" => Ok(tts_protocol::Device::Auto),
        "cpu" => Ok(tts_protocol::Device::Cpu),
        "coreml" => Ok(tts_protocol::Device::Coreml),
        "cuda" => Ok(tts_protocol::Device::Cuda),
        _ => Err("expected auto/cpu/coreml/cuda".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn alignment_flag_keeps_file_argument_and_supports_explicit_disable() {
        let enabled = Args::try_parse_from(["novel-tts", "--alignment", "book.txt"]).unwrap();
        assert_eq!(enabled.alignment, Some(true));
        assert_eq!(enabled.file, Some(PathBuf::from("book.txt")));
        let disabled =
            Args::try_parse_from(["novel-tts", "--alignment=false", "book.txt"]).unwrap();
        assert_eq!(disabled.alignment, Some(false));
    }
}
