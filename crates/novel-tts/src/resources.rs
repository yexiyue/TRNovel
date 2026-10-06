use std::{path::PathBuf, rc::Rc};
use tokio::sync::mpsc;
use tts_core::{CheckpointModel, VoicesData, backend::KokoroBackend};
use tts_protocol::Event;

#[derive(Clone)]
pub struct Resources {
    model: CheckpointModel,
    voices: VoicesData,
}

impl Resources {
    pub fn new(directory: Option<PathBuf>) -> Self {
        match directory {
            Some(directory) => Self {
                model: CheckpointModel::new(directory.join("kokoro-v1.1-zh.onnx")),
                voices: VoicesData::new(directory.join("voices-v1.1-zh.bin")),
            },
            None => Self {
                model: CheckpointModel::default(),
                voices: VoicesData::default(),
            },
        }
    }

    pub async fn prepare(
        mut self,
        progress: mpsc::Sender<Event>,
    ) -> anyhow::Result<Rc<KokoroBackend>> {
        if !self.model.is_downloaded() {
            let tx = progress.clone();
            self.model
                .async_download(move |downloaded, total| {
                    // Progress is advisory. A slow parent must not block downloads
                    // or allocate an unbounded number of progress messages.
                    let _ = tx.try_send(Event::ModelProgress {
                        resource: "model".into(),
                        downloaded,
                        total,
                    });
                })
                .await?;
        }
        if !self.voices.is_downloaded() {
            self.voices
                .async_download(move |downloaded, total| {
                    let _ = progress.try_send(Event::ModelProgress {
                        resource: "voices".into(),
                        downloaded,
                        total,
                    });
                })
                .await?;
        }
        Ok(Rc::new(
            KokoroBackend::load(&self.model.path, &self.voices.path).await?,
        ))
    }
}
