use crate::resources::Resources;
use clap::Subcommand;
use std::path::PathBuf;
#[derive(Subcommand)]
pub enum VoiceCommand {
    /// List built-in and imported MOSS voices.
    List,
    /// Encode a WAV reference once. ID accepts ASCII letters, digits, '-' and '_'.
    Import {
        id: String,
        #[arg(long)]
        name: String,
        wav: PathBuf,
    },
    /// Delete an imported voice; built-in voices cannot be deleted.
    Remove { id: String },
}
pub async fn run(command: VoiceCommand, resources: Resources) -> anyhow::Result<()> {
    #[cfg(feature = "moss")]
    {
        let directory = resources.root().join("moss");
        match command {
            VoiceCommand::List => {
                let caps = resources.capabilities("moss")?;
                for id in caps.voices {
                    println!(
                        "{}\t{}",
                        id,
                        caps.voice_names
                            .get(&id)
                            .map_or(id.as_str(), String::as_str)
                    );
                }
            }
            VoiceCommand::Remove { id } => tts_backends::moss::voices::VoiceStore::new(&directory)
                .remove(&format!("custom:{}", id.trim_start_matches("custom:")))?,
            VoiceCommand::Import { id, name, wav } => {
                let id = format!("custom:{}", id.trim_start_matches("custom:"));
                let (progress, mut rx) = tokio::sync::mpsc::channel(16);
                let logger = tokio::task::spawn_local(async move {
                    while let Some(event) = rx.recv().await {
                        if let tts_protocol::Event::ModelProgress {
                            resource,
                            downloaded,
                            total,
                        } = event
                        {
                            eprintln!("{resource}: {downloaded}/{total}");
                        }
                    }
                });
                let result = async {
                    tts_backends::moss::resources::prepare(&directory, progress).await?;
                    let backend = tts_backends::moss::MossBackend::load(directory).await?;
                    backend.import_voice(id.clone(), name, wav).await
                }
                .await;
                logger.abort();
                result?;
                eprintln!("Imported {id}");
            }
        }
        Ok(())
    }
    #[cfg(not(feature = "moss"))]
    {
        let _ = (command, resources);
        anyhow::bail!("voice import requires the moss Cargo feature")
    }
}
