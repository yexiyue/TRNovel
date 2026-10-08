//! One transactional TOML store for application preferences.
use crate::{
    cache::{AppearanceConfig, ReaderDisplayConfig},
    paths::AppPaths,
};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{ErrorKind, Write},
    path::PathBuf,
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub appearance: AppearanceConfig,
    pub reader: ReaderDisplayConfig,
    pub browser: BrowserConfig,
    pub tts: TtsConfig,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BrowserConfig {
    pub always_allow: bool,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct TtsConfig {
    pub program: Option<PathBuf>,
}

pub struct ConfigStore {
    path: PathBuf,
}
impl ConfigStore {
    pub fn new(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn user_default() -> anyhow::Result<Self> {
        Ok(Self::new(AppPaths::user_default()?.config()))
    }
    pub fn load(&self) -> anyhow::Result<AppConfig> {
        let mut config: AppConfig = match fs::read_to_string(&self.path) {
            Ok(text) => toml::from_str(&text)?,
            Err(error) if error.kind() == ErrorKind::NotFound => AppConfig::default(),
            Err(error) => return Err(error.into()),
        };
        config.reader.normalize();
        Ok(config)
    }
    /// Lock, reload and change one section before atomically replacing the file.
    /// Never write a cached snapshot of unrelated preferences.
    pub fn update(&self, change: impl FnOnce(&mut AppConfig)) -> anyhow::Result<()> {
        let parent = self
            .path
            .parent()
            .ok_or_else(|| anyhow::anyhow!("配置文件没有父目录"))?;
        fs::create_dir_all(parent)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.path.with_extension("lock"))?;
        lock.lock()?;
        let mut config = self.load()?;
        change(&mut config);
        config.reader.normalize();
        let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
        temporary.write_all(toml::to_string_pretty(&config)?.as_bytes())?;
        temporary.as_file().sync_all()?;
        temporary.persist(&self.path)?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn defaults_ignore_old_files_without_creating_new_home() {
        let home = tempfile::tempdir().unwrap();
        let legacy = home.path().join(".novel/appearance.json");
        fs::create_dir_all(legacy.parent().unwrap()).unwrap();
        fs::write(&legacy, "invalid legacy bytes").unwrap();
        let paths = AppPaths::from_home(home.path());
        let config = ConfigStore::new(paths.config()).load().unwrap();
        assert_eq!(config.reader, ReaderDisplayConfig::default());
        assert!(!paths.root().exists());
        assert_eq!(fs::read_to_string(legacy).unwrap(), "invalid legacy bytes");
    }
    #[test]
    fn alternating_updates_preserve_other_sections_and_validate_reader() {
        let home = tempfile::tempdir().unwrap();
        let path = AppPaths::from_home(home.path()).config();
        let first = ConfigStore::new(path.clone());
        let second = ConfigStore::new(path.clone());
        first
            .update(|c| c.appearance.theme_slug = "dracula".into())
            .unwrap();
        second
            .update(|c| {
                c.reader.show_title = false;
                c.reader.page_overlap = 100;
            })
            .unwrap();
        first.update(|c| c.browser.always_allow = true).unwrap();
        second
            .update(|c| c.tts.program = Some("custom worker".into()))
            .unwrap();
        first
            .update(|c| c.appearance.background = crate::cache::BackgroundMode::Theme)
            .unwrap();
        let c = first.load().unwrap();
        assert_eq!(c.appearance.theme_slug, "dracula");
        assert!(!c.reader.show_title);
        assert_eq!(c.reader.page_overlap, 10);
        assert!(c.browser.always_allow);
        assert_eq!(c.tts.program, Some(PathBuf::from("custom worker")));
        fs::write(&path, "[invalid").unwrap();
        assert!(first.update(|c| c.browser.always_allow = false).is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), "[invalid");
    }
}
