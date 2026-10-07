//! Concrete model adapters. The core and reader never import inference types.
#[cfg(feature = "alignment")]
pub mod alignment;
#[cfg(any(
    feature = "moss",
    feature = "alignment",
    feature = "qwen",
    feature = "voxcpm",
    feature = "omnivoice",
))]
pub mod devices;
#[cfg(feature = "moss")]
pub mod moss;
#[cfg(feature = "omnivoice")]
pub mod omnivoice;
#[cfg(feature = "qwen")]
pub mod qwen;
#[cfg(any(feature = "voxcpm", feature = "omnivoice", feature = "moss-candle"))]
pub mod reference;
#[cfg(any(
    feature = "moss",
    feature = "alignment",
    feature = "qwen",
    feature = "voxcpm",
    feature = "omnivoice",
))]
mod resources;
#[cfg(feature = "voxcpm")]
pub mod voxcpm;

#[cfg(feature = "omnivoice-onnx")]
pub mod omnivoice_onnx;
#[cfg(any(feature = "qwen-onnx", feature = "omnivoice-onnx"))]
mod onnx_runtime;
#[cfg(feature = "qwen-onnx")]
pub mod qwen_onnx;

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
        feature = "qwen",
        feature = "voxcpm",
        feature = "omnivoice",
    ))]
    fn devices_for(&self, backend: &str, available: bool) -> Vec<tts_protocol::Device> {
        let _ = available;
        match backend {
            #[cfg(any(feature = "qwen-onnx", feature = "omnivoice-onnx"))]
            "qwen-onnx" | "omnivoice-onnx" => {
                if available {
                    devices::available()
                } else {
                    devices::compiled()
                }
            }
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
        feature = "qwen",
        feature = "voxcpm",
        feature = "omnivoice",
    ))]
    pub fn compiled_devices(&self, backend: &str) -> Vec<tts_protocol::Device> {
        self.devices_for(backend, false)
    }
    #[cfg(any(
        feature = "moss",
        feature = "qwen",
        feature = "voxcpm",
        feature = "omnivoice",
    ))]
    pub fn available_devices(&self, backend: &str) -> Vec<tts_protocol::Device> {
        self.devices_for(backend, true)
    }
    #[cfg(any(
        feature = "moss",
        feature = "qwen",
        feature = "voxcpm",
        feature = "omnivoice",
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
        feature = "qwen",
        feature = "voxcpm",
        feature = "omnivoice",
    ))]
    pub fn compiled_devices_for(
        &self,
        backend: &str,
        model: Option<&str>,
    ) -> Vec<tts_protocol::Device> {
        #[cfg(feature = "moss-nano-candle")]
        if backend == "moss" && model == Some("nano-candle") {
            return moss::nano::compiled_devices();
        }
        #[cfg(feature = "moss-candle")]
        if backend == "moss" && model.is_some_and(|id| id != "nano") {
            return moss::candle::compiled_devices();
        }
        let _ = model;
        self.compiled_devices(backend)
    }
    #[cfg(any(
        feature = "moss",
        feature = "qwen",
        feature = "voxcpm",
        feature = "omnivoice",
    ))]
    pub fn available_devices_for(
        &self,
        backend: &str,
        model: Option<&str>,
    ) -> Vec<tts_protocol::Device> {
        #[cfg(feature = "moss-nano-candle")]
        if backend == "moss" && model == Some("nano-candle") {
            return moss::nano::available_devices();
        }
        #[cfg(feature = "moss-candle")]
        if backend == "moss" && model.is_some_and(|id| id != "nano") {
            return moss::candle::available_devices();
        }
        let _ = model;
        self.available_devices(backend)
    }
    #[cfg(any(
        feature = "moss",
        feature = "qwen",
        feature = "voxcpm",
        feature = "omnivoice",
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
            #[cfg(feature = "moss-nano-candle")]
            moss::nano::capabilities(&self.root)?,
            #[cfg(feature = "moss-candle")]
            moss::candle::capabilities(&self.root, moss::candle::Mode::Local)?,
            #[cfg(feature = "moss-candle")]
            moss::candle::capabilities(&self.root, moss::candle::Mode::Realtime)?,
            #[cfg(feature = "voxcpm")]
            voxcpm::capabilities(&voxcpm::directory(&self.root))?,
            #[cfg(feature = "omnivoice")]
            omnivoice::capabilities(&omnivoice::directory(&self.root))?,
        ];
        #[cfg(feature = "qwen")]
        let entries = entries
            .into_iter()
            .chain(qwen::catalog(&self.root)?)
            .collect();
        #[allow(unused_mut)] // An empty-backend build has no device catalog to populate.
        let mut entries: Vec<Capabilities> = entries;
        #[cfg(feature = "qwen-onnx")]
        entries.extend([
            qwen_onnx::capabilities(qwen::models::Model::Custom06),
            qwen_onnx::capabilities(qwen::models::Model::Custom17),
        ]);
        #[cfg(feature = "omnivoice-onnx")]
        entries.push(omnivoice_onnx::capabilities(&self.root)?);
        #[cfg(any(
            feature = "moss",
            feature = "qwen",
            feature = "voxcpm",
            feature = "omnivoice",
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
        if let Some(caps) = catalog.iter().find(|caps| {
            caps.backend == "moss" && caps.model.as_deref().is_none_or(|id| id == "nano")
        }) {
            return Ok(tts_protocol::Config {
                backend: caps.backend.clone(),
                model: caps.model.clone(),
                voice: caps.default_voice.clone(),
                tts_device: tts_protocol::Device::Cpu,
                ..Default::default()
            });
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
            feature = "qwen",
            feature = "voxcpm",
            feature = "omnivoice",
        ))]
        anyhow::ensure!(
            self.available_devices_for(id, model).contains(&device),
            "device {device:?} is unavailable for {id}"
        );
        match id {
            #[cfg(feature = "omnivoice-onnx")]
            "omnivoice-onnx" => {
                omnivoice_onnx::prepare(&self.root, progress).await?;
                Ok(Rc::new(omnivoice_onnx::load(&self.root, device).await?))
            }
            #[cfg(feature = "qwen-onnx")]
            "qwen-onnx" => {
                let model = qwen_onnx::model(model)?;
                qwen_onnx::prepare(&self.root, model, progress).await?;
                Ok(Rc::new(qwen_onnx::load(&self.root, model, device).await?))
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
                #[cfg(feature = "moss-nano-candle")]
                if model == Some("nano-candle") {
                    moss::nano::prepare(&self.root, progress).await?;
                    return Ok(Rc::new(
                        moss::nano::NanoBackend::load_on(self.root.clone(), device).await?,
                    ));
                }
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
            _ => anyhow::bail!("backend {id} is not compiled"),
        }
    }
}
