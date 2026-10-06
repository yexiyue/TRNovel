//! A small consumer of the model-independent session interface.
use novel_tts_core as tts_core;
use std::rc::Rc;
use tts_core::{
    AudioPlayer, CheckpointModel, VoicesData, backend::KokoroBackend, checkpoint::CheckpointStore,
    session::SessionManager,
};
use tts_protocol::{Config, Event, SourceId, StartRequest, text_hash};

#[tokio::main(flavor = "current_thread")]
async fn main() -> anyhow::Result<()> {
    tokio::task::LocalSet::new()
        .run_until(async {
            let mut model = CheckpointModel::default();
            let mut voices = VoicesData::default();
            for resource in [&mut *model, &mut *voices] {
                if !resource.is_downloaded() {
                    resource
                        .async_download(|done, total| eprintln!("{done}/{total}"))
                        .await?;
                }
            }
            let backend = Rc::new(KokoroBackend::load(&model.path, &voices.path).await?);
            let (events, mut rx) = tokio::sync::mpsc::channel(64);
            let mut session = SessionManager::new(
                backend,
                Rc::new(AudioPlayer::open()?),
                CheckpointStore::user_default()?,
                events,
            );
            let text = "清晨的风吹过窗台。今天，我们一起读书。";
            session
                .start(
                    "example".into(),
                    StartRequest {
                        source: SourceId {
                            namespace: "example".into(),
                            book: "basic".into(),
                            chapter: "1".into(),
                        },
                        text: text.into(),
                        text_hash: text_hash(text),
                        resume_byte: None,
                        restore_checkpoint: false,
                    },
                    &Config::default(),
                )
                .await?;
            while let Some(event) = rx.recv().await {
                eprintln!("{:?}", event.event);
                if matches!(event.event, Event::SessionEnded { .. }) {
                    break;
                }
            }
            session.stop().await?;
            Ok(())
        })
        .await
}
