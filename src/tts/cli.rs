//! Non-TUI listening configuration; model catalogs never prepare or play audio.
use super::client::{Client, discover};
use crate::config::{ConfigStore, SoundConfig, TtsConfig};
use clap::{Args, Subcommand, ValueEnum};
use std::path::{Path, PathBuf};
use tts_protocol::{Capabilities, Command, Device, Event};

#[derive(Debug, Clone, Hash, PartialEq, Eq, Subcommand)]
pub enum TtsCommand {
    /// 查看阅读器保存的听书配置，无需启动 Talechime
    Show,
    /// 列出模型、音色和编译支持的设备，不下载模型
    List {
        #[arg(long)]
        backend: Option<String>,
        #[arg(long, requires = "backend")]
        model: Option<String>,
    },
    /// 校验并保存下一次播放的设置，不下载模型
    Setup(SetupArgs),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Default, Args)]
pub struct SetupArgs {
    /// 独立 Talechime 可执行文件路径（保存为绝对路径）
    #[arg(long, conflicts_with = "tts_program")]
    pub program: Option<PathBuf>,
    #[arg(long)]
    pub backend: Option<String>,
    #[arg(long)]
    pub model: Option<String>,
    /// 音色 ID；使用 tts list 查看
    #[arg(long)]
    pub voice: Option<String>,
    /// 朗读风格；空字符串清除风格
    #[arg(long)]
    pub style: Option<String>,
    /// 合成设备，受 worker 编译能力限制
    #[arg(long, value_enum)]
    pub device: Option<SetupDevice>,
}

#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq, ValueEnum)]
pub enum SetupDevice {
    Auto,
    Cpu,
    Metal,
    Cuda,
    Coreml,
}
impl From<SetupDevice> for Device {
    fn from(value: SetupDevice) -> Self {
        match value {
            SetupDevice::Auto => Self::Auto,
            SetupDevice::Cpu => Self::Cpu,
            SetupDevice::Metal => Self::Metal,
            SetupDevice::Cuda => Self::Cuda,
            SetupDevice::Coreml => Self::Coreml,
        }
    }
}
impl SetupArgs {
    fn changes_sound(&self) -> bool {
        self.backend.is_some()
            || self.model.is_some()
            || self.voice.is_some()
            || self.style.is_some()
            || self.device.is_some()
    }
    fn sound(&self, base: &SoundConfig, catalog: &[Capabilities]) -> anyhow::Result<SoundConfig> {
        let backend = self.backend.as_deref().unwrap_or(&base.backend);
        let model = match &self.model {
            Some(model) => Some(model.as_str()),
            None if backend == base.backend => base.model.as_deref(),
            None => None,
        };
        let caps = catalog
            .iter()
            .find(|caps| caps.matches(backend, model))
            .ok_or_else(|| {
                anyhow::anyhow!(
                    "不支持的模型: {backend}/{}；请使用 tts list 查询",
                    model.unwrap_or("default")
                )
            })?;
        let changed = catalog
            .iter()
            .find(|previous| previous.matches(&base.backend, base.model.as_deref()))
            .is_none_or(|previous| {
                previous.backend != caps.backend || previous.model != caps.model
            });
        let voice = self.voice.clone().unwrap_or_else(|| {
            if changed {
                caps.default_voice.clone()
            } else {
                base.voice.clone()
            }
        });
        anyhow::ensure!(
            caps.voices.contains(&voice),
            "不支持的音色 ID: {voice}；请使用 tts list 查询"
        );
        let style = match &self.style {
            Some(style) => (!style.trim().is_empty()).then(|| style.clone()),
            None if changed => None,
            None => base.style.clone(),
        };
        anyhow::ensure!(style.is_none() || caps.style, "所选模型不支持朗读风格");
        anyhow::ensure!(
            style
                .as_ref()
                .is_none_or(|style| style.chars().count() <= 200),
            "朗读风格不能超过 200 个字符"
        );
        let mut device = self.device.map(Device::from).unwrap_or(base.device);
        if changed && self.device.is_none() && !caps.compiled_devices.contains(&device) {
            device = Device::Auto;
        }
        anyhow::ensure!(
            device == Device::Auto || caps.compiled_devices.contains(&device),
            "所选模型未编译设备 {device:?}；请使用 tts list 查询"
        );
        Ok(SoundConfig {
            backend: caps.backend.clone(),
            model: caps.model.clone(),
            voice,
            style,
            device,
        })
    }
}

fn print_config(config: &TtsConfig) -> anyhow::Result<()> {
    #[derive(serde::Serialize)]
    struct Display<'a> {
        tts: &'a TtsConfig,
    }
    println!("{}", toml::to_string_pretty(&Display { tts: config })?);
    Ok(())
}

pub async fn run(command: &TtsCommand, explicit: Option<&Path>) -> anyhow::Result<()> {
    execute(command, explicit, &ConfigStore::user_default()?).await
}
async fn execute(
    command: &TtsCommand,
    explicit: Option<&Path>,
    store: &ConfigStore,
) -> anyhow::Result<()> {
    let saved = store.load()?.tts;
    if matches!(command, TtsCommand::Show) {
        return print_config(&saved);
    }
    let setup = match command {
        TtsCommand::Setup(args) => Some(args),
        _ => None,
    };
    if let Some(args) = setup {
        anyhow::ensure!(
            args.program.is_none() || explicit.is_none(),
            "--program 和 --tts-program 只能指定一个"
        );
        anyhow::ensure!(
            args.program.is_some() || explicit.is_some() || args.changes_sound(),
            "请指定至少一项设置；使用 tts setup --help 查看参数或 tts show 查看配置"
        );
    }
    let requested = setup.and_then(|args| args.program.as_deref()).or(explicit);
    let program = discover(
        requested,
        saved.program.as_deref(),
        std::env::var_os("PATH").as_deref(),
    )?
    .canonicalize()?;
    let (client, ready) = Client::connect(&program).await?;
    let result = async {
        let Event::Ready(catalog) = ready.event else {
            anyhow::bail!("worker 未返回模型目录");
        };
        match command {
            TtsCommand::List { backend, model } => {
                let mut entries: Vec<_> = catalog
                    .iter()
                    .filter(|caps| {
                        backend
                            .as_ref()
                            .is_none_or(|backend| caps.backend == *backend)
                            && model
                                .as_ref()
                                .is_none_or(|model| caps.model.as_ref() == Some(model))
                    })
                    .collect();
                anyhow::ensure!(
                    !entries.is_empty(),
                    "没有匹配的模型；请使用 tts list 查询全部目录"
                );
                entries.sort_by_key(|caps| (&caps.backend, &caps.model));
                for caps in entries {
                    println!(
                        "{} / {} — {}",
                        caps.backend,
                        caps.model.as_deref().unwrap_or("default"),
                        caps.model_name
                    );
                    println!(
                        "  设备: auto, {:?}；风格: {}",
                        caps.compiled_devices,
                        if caps.style { "支持" } else { "不支持" }
                    );
                    for voice in &caps.voices {
                        println!(
                            "  {voice}: {}",
                            caps.voice_names.get(voice).unwrap_or(voice)
                        );
                    }
                }
            }
            TtsCommand::Setup(args) => {
                let sound = if args.changes_sound() {
                    let base = match &saved.sound {
                        Some(sound) => sound.clone(),
                        None => {
                            let response = client.command(None, Command::GetConfig).await?;
                            let Event::Config(config) = response.event else {
                                anyhow::bail!("worker 未返回声音配置");
                            };
                            SoundConfig::from(&config)
                        }
                    };
                    Some(args.sound(&base, &catalog)?)
                } else {
                    None
                };
                store.try_update(|config| {
                    anyhow::ensure!(
                        config.tts.program == saved.program && config.tts.sound == saved.sound,
                        "听书配置已被其他进程修改，请重新执行设置"
                    );
                    if requested.is_some() {
                        config.tts.program = Some(program.clone());
                    }
                    if let Some(sound) = sound {
                        config.tts.sound = Some(sound);
                    }
                    Ok(())
                })?;
                println!("已保存，下次播放生效。worker: {}", program.display());
                print_config(&store.load()?.tts)?;
            }
            TtsCommand::Show => unreachable!(),
        }
        Ok(())
    }
    .await;
    client.close().await;
    result
}

#[cfg(test)]
mod tests;
