//! Concrete model adapters. The core and reader never import inference types.
#[cfg(feature = "alignment")]
pub mod alignment;
#[cfg(any(
    feature = "moss",
    feature = "alignment",
    feature = "kokoro",
    feature = "qwen",
    feature = "voxcpm",
    feature = "omnivoice",
    feature = "zipvoice"
))]
pub mod devices;
#[cfg(feature = "kokoro")]
pub mod kokoro;
#[cfg(feature = "moss")]
pub mod moss;
#[cfg(feature = "omnivoice")]
pub mod omnivoice;
#[cfg(feature = "qwen")]
pub mod qwen;
#[cfg(any(
    feature = "voxcpm",
    feature = "omnivoice",
    feature = "zipvoice",
    feature = "moss-candle"
))]
pub mod reference;
#[cfg(any(
    feature = "moss",
    feature = "alignment",
    feature = "kokoro",
    feature = "qwen",
    feature = "voxcpm",
    feature = "omnivoice",
    feature = "zipvoice"
))]
mod resources;
#[cfg(feature = "voxcpm")]
pub mod voxcpm;
#[cfg(feature = "zipvoice")]
pub mod zipvoice;

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
    #[cfg(any(
        feature = "moss",
        feature = "kokoro",
        feature = "qwen",
        feature = "voxcpm",
        feature = "omnivoice",
        feature = "zipvoice"
    ))]
    fn devices_for(&self, backend: &str, available: bool) -> Vec<tts_protocol::Device> {
        let _ = available;
        match backend {
            #[cfg(feature = "zipvoice")]
            "zipvoice" => vec![tts_protocol::Device::Cpu],
            #[cfg(feature = "voxcpm")]
            "voxcpm" => {
                if available {
                    voxcpm::available_devices()
                } else {
                    voxcpm::compiled_devices()
                }
            }
            #[cfg(feature = "omnivoice")]
            "omnivoice" => {
                if available {
                    omnivoice::available_devices()
                } else {
                    omnivoice::compiled_devices()
                }
            }
            #[cfg(feature = "qwen")]
            "qwen" => {
                if available {
                    qwen::available_devices()
                } else {
                    qwen::compiled_devices()
                }
            }
            #[cfg(feature = "kokoro")]
            "kokoro" => vec![tts_protocol::Device::Cpu],
            #[cfg(feature = "moss")]
            "moss" => {
                if available {
                    devices::available()
                } else {
                    devices::compiled()
                }
            }
            _ => vec![],
        }
    }
    #[cfg(any(
        feature = "moss",
        feature = "kokoro",
        feature = "qwen",
        feature = "voxcpm",
        feature = "omnivoice",
        feature = "zipvoice"
    ))]
    pub fn compiled_devices(&self, backend: &str) -> Vec<tts_protocol::Device> {
        self.devices_for(backend, false)
    }
    #[cfg(any(
        feature = "moss",
        feature = "kokoro",
        feature = "qwen",
        feature = "voxcpm",
        feature = "omnivoice",
        feature = "zipvoice"
    ))]
    pub fn available_devices(&self, backend: &str) -> Vec<tts_protocol::Device> {
        self.devices_for(backend, true)
    }
    #[cfg(any(
        feature = "moss",
        feature = "kokoro",
        feature = "qwen",
        feature = "voxcpm",
        feature = "omnivoice",
        feature = "zipvoice"
    ))]
    pub fn device_status(
        &self,
        backend: &str,
        selected: tts_protocol::Device,
        reason: Option<String>,
    ) -> Event {
        Event::DeviceStatus {
            component: "tts".into(),
            compiled: self.compiled_devices(backend),
            available: self.available_devices(backend),
            selected,
            reason,
        }
    }
    #[cfg(any(
        feature = "moss",
        feature = "kokoro",
        feature = "qwen",
        feature = "voxcpm",
        feature = "omnivoice",
        feature = "zipvoice"
    ))]
    pub fn compiled_devices_for(
        &self,
        backend: &str,
        model: Option<&str>,
    ) -> Vec<tts_protocol::Device> {
        #[cfg(feature = "moss-candle")]
        if backend == "moss" && model.is_some_and(|id| id != "nano") {
            return moss::candle::compiled_devices();
        }
        let _ = model;
        self.compiled_devices(backend)
    }
    #[cfg(any(
        feature = "moss",
        feature = "kokoro",
        feature = "qwen",
        feature = "voxcpm",
        feature = "omnivoice",
        feature = "zipvoice"
    ))]
    pub fn available_devices_for(
        &self,
        backend: &str,
        model: Option<&str>,
    ) -> Vec<tts_protocol::Device> {
        #[cfg(feature = "moss-candle")]
        if backend == "moss" && model.is_some_and(|id| id != "nano") {
            return moss::candle::available_devices();
        }
        let _ = model;
        self.available_devices(backend)
    }
    #[cfg(any(
        feature = "moss",
        feature = "kokoro",
        feature = "qwen",
        feature = "voxcpm",
        feature = "omnivoice",
        feature = "zipvoice"
    ))]
    pub fn device_status_for(
        &self,
        backend: &str,
        model: Option<&str>,
        selected: tts_protocol::Device,
        reason: Option<String>,
    ) -> Event {
        Event::DeviceStatus {
            component: "tts".into(),
            compiled: self.compiled_devices_for(backend, model),
            available: self.available_devices_for(backend, model),
            selected,
            reason,
        }
    }
    pub fn catalog(&self) -> anyhow::Result<Vec<Capabilities>> {
        let entries = vec![
            #[cfg(feature = "moss")]
            moss::capabilities(&self.root.join("moss"))?,
            #[cfg(feature = "moss-candle")]
            moss::candle::capabilities(&self.root, moss::candle::Mode::Local)?,
            #[cfg(feature = "moss-candle")]
            moss::candle::capabilities(&self.root, moss::candle::Mode::Realtime)?,
            #[cfg(feature = "kokoro")]
            kokoro::KokoroBackend::capabilities(),
            #[cfg(feature = "voxcpm")]
            voxcpm::capabilities(&voxcpm::directory(&self.root))?,
            #[cfg(feature = "omnivoice")]
            omnivoice::capabilities(&omnivoice::directory(&self.root))?,
            #[cfg(feature = "zipvoice")]
            zipvoice::capabilities(
                &zipvoice::Variant::Int8.directory(&self.root),
                zipvoice::Variant::Int8,
            )?,
            #[cfg(feature = "zipvoice")]
            zipvoice::capabilities(
                &zipvoice::Variant::Fp32.directory(&self.root),
                zipvoice::Variant::Fp32,
            )?,
        ];
        #[cfg(feature = "qwen")]
        let entries = entries
            .into_iter()
            .chain(qwen::catalog(&self.root)?)
            .collect();
        #[allow(unused_mut)] // An empty-backend build has no device catalog to populate.
        let mut entries: Vec<Capabilities> = entries;
        #[cfg(any(
            feature = "moss",
            feature = "kokoro",
            feature = "qwen",
            feature = "voxcpm",
            feature = "omnivoice",
            feature = "zipvoice"
        ))]
        for caps in &mut entries {
            caps.compiled_devices = self.compiled_devices_for(&caps.backend, caps.model.as_deref());
        }
        Ok(entries)
    }
    pub fn capabilities(&self, id: &str) -> anyhow::Result<Capabilities> {
        self.capabilities_for(id, None)
    }
    pub fn capabilities_for(&self, id: &str, model: Option<&str>) -> anyhow::Result<Capabilities> {
        self.catalog()?
            .into_iter()
            .find(|entry| entry.matches(id, model))
            .ok_or_else(|| {
                anyhow::anyhow!("backend/model {id}/{model:?} is unavailable in this build")
            })
    }
    /// Resolve defaults only for a new settings file; existing users retain their choices.
    pub fn default_config(&self) -> anyhow::Result<tts_protocol::Config> {
        let catalog = self.catalog()?;
        #[cfg(feature = "qwen")]
        if let Some(device) = qwen::available_devices()
            .iter()
            .copied()
            .find(|d| matches!(d, tts_protocol::Device::Cuda | tts_protocol::Device::Metal))
        {
            return Ok(tts_protocol::Config {
                backend: "qwen".into(),
                model: Some(qwen::models::Model::Custom17.id().into()),
                voice: qwen::capabilities().default_voice,
                tts_device: device,
                ..Default::default()
            });
        }
        for id in ["moss", "kokoro", "qwen"] {
            if let Some(caps) = catalog.iter().find(|caps| caps.backend == id) {
                return Ok(tts_protocol::Config {
                    backend: caps.backend.clone(),
                    model: caps.model.clone(),
                    voice: caps.default_voice.clone(),
                    tts_device: tts_protocol::Device::Cpu,
                    ..Default::default()
                });
            }
        }
        anyhow::bail!(
            "no default synthesis backend is compiled; select an available backend explicitly"
        )
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
        self.prepare_model_on(id, None, progress, device).await
    }
    pub async fn prepare_model_on(
        &self,
        id: &str,
        model: Option<&str>,
        progress: tokio::sync::mpsc::Sender<Event>,
        device: tts_protocol::Device,
    ) -> anyhow::Result<Rc<dyn Backend>> {
        self.capabilities_for(id, model)?;
        let _ = (&progress, device);
        #[cfg(any(
            feature = "moss",
            feature = "kokoro",
            feature = "qwen",
            feature = "voxcpm",
            feature = "omnivoice",
            feature = "zipvoice"
        ))]
        anyhow::ensure!(
            self.available_devices_for(id, model).contains(&device),
            "device {device:?} is unavailable for {id}"
        );
        match id {
            #[cfg(feature = "zipvoice")]
            "zipvoice" => {
                let variant = zipvoice::Variant::parse(model)?;
                let directory = variant.directory(&self.root);
                zipvoice::resources::prepare(&directory, variant, progress).await?;
                Ok(Rc::new(
                    zipvoice::ZipBackend::load_on(directory, variant, device).await?,
                ))
            }
            #[cfg(feature = "voxcpm")]
            "voxcpm" => {
                let directory = voxcpm::directory(&self.root);
                voxcpm::resources::prepare(&directory, progress).await?;
                Ok(Rc::new(
                    voxcpm::VoxBackend::load_on(directory, device).await?,
                ))
            }
            #[cfg(feature = "omnivoice")]
            "omnivoice" => {
                let directory = omnivoice::directory(&self.root);
                omnivoice::resources::prepare(&directory, progress).await?;
                Ok(Rc::new(
                    omnivoice::OmniBackend::load_on(directory, device).await?,
                ))
            }
            #[cfg(feature = "qwen")]
            "qwen" => {
                let model = qwen::models::Model::parse(model)?;
                let directory = model.directory(&self.root);
                qwen::resources::prepare_model(&directory, model, progress).await?;
                Ok(Rc::new(
                    qwen::QwenBackend::load_on(directory, device).await?,
                ))
            }
            #[cfg(feature = "moss")]
            "moss" => {
                #[cfg(feature = "moss-candle")]
                if let Some(model) = model.filter(|id| *id != "nano") {
                    let mode = moss::candle::Mode::parse(model)?;
                    moss::candle::resources::prepare(&self.root, mode, progress).await?;
                    return Ok(Rc::new(
                        moss::candle::CandleBackend::load_on(self.root.clone(), mode, device)
                            .await?,
                    ));
                }
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
