//! Concrete model adapters. The core and reader never import inference types.
#[cfg(feature = "alignment")]
pub mod alignment;
#[cfg(any(feature = "moss", feature = "alignment", feature = "kokoro"))]
pub mod devices;
#[cfg(feature = "kokoro")]
pub mod kokoro;
#[cfg(feature = "moss")]
pub mod moss;
#[cfg(any(feature = "moss", feature = "alignment", feature = "kokoro"))]
mod resources;

use std::{
    path::{Path, PathBuf},
    rc::Rc,
};
use tts_core::backend::Backend;
use tts_protocol::{Capabilities, Event};

#[derive(Clone)]
pub struct Registry {
    root: PathBuf,
}
impl Registry {
    pub fn new(root: Option<PathBuf>) -> anyhow::Result<Self> {
        Ok(Self {
            root: root.map_or_else(tts_core::download::get_cache_dir, Ok)?,
        })
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn catalog(&self) -> anyhow::Result<Vec<Capabilities>> {
        let entries = vec![
            #[cfg(feature = "moss")]
            moss::capabilities(&self.root.join("moss"))?,
            #[cfg(feature = "kokoro")]
            kokoro::KokoroBackend::capabilities(),
        ];
        Ok(entries)
    }
    pub fn capabilities(&self, id: &str) -> anyhow::Result<Capabilities> {
        self.catalog()?.into_iter().find(|entry| entry.backend == id)
            .ok_or_else(|| anyhow::anyhow!("backend {id} is not compiled; enable its Cargo feature or select an available backend"))
    }
    pub async fn prepare(
        &self,
        id: &str,
        progress: tokio::sync::mpsc::Sender<Event>,
    ) -> anyhow::Result<Rc<dyn Backend>> {
        self.prepare_on(id, progress, tts_protocol::Device::Cpu)
            .await
    }
    pub async fn prepare_on(
        &self,
        id: &str,
        progress: tokio::sync::mpsc::Sender<Event>,
        device: tts_protocol::Device,
    ) -> anyhow::Result<Rc<dyn Backend>> {
        let _ = (&progress, device);
        match id {
            #[cfg(feature = "moss")]
            "moss" => {
                let directory = self.root.join("moss");
                moss::resources::prepare(&directory, progress).await?;
                Ok(Rc::new(
                    moss::MossBackend::load_on(directory, device).await?,
                ))
            }
            #[cfg(feature = "kokoro")]
            "kokoro" => {
                anyhow::ensure!(
                    device == tts_protocol::Device::Cpu,
                    "Kokoro currently supports CPU only"
                );
                let directory = self.root.join("kokoro");
                kokoro::models::prepare(&directory, progress).await?;
                Ok(Rc::new(
                    kokoro::KokoroBackend::load(
                        &directory.join("kokoro-v1.1-zh.onnx"),
                        &directory.join("voices-v1.1-zh.bin"),
                    )
                    .await?,
                ))
            }
            _ => anyhow::bail!("backend {id} is not compiled"),
        }
    }
}
