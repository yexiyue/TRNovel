//! Listening configuration. Only the listening process writes this file.
use std::{
    fs::File,
    path::{Path, PathBuf},
};
use tts_protocol::{Capabilities, Config, ConfigPatch};

/// Loading, validation and update errors never replace user data with defaults.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    #[error("configuration IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid configuration: {0}")]
    Json(#[from] serde_json::Error),
    #[error("configuration revision changed; refresh before retrying")]
    RevisionConflict,
    #[error("unsupported or invalid setting: {0}")]
    Invalid(String),
}

/// Existing `~/.novel/tts_config.json` ownership, without a Drop save hook.
#[derive(Debug, Clone)]
pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    /// Use an explicit path, including an isolated path in tests.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The legacy path, retained for existing users.
    pub fn user_default() -> Result<Self, ConfigError> {
        let home = dirs::home_dir()
            .ok_or_else(|| ConfigError::Invalid("home directory unavailable".into()))?;
        Ok(Self::new(home.join(".novel/tts_config.json")))
    }

    /// Path of the shared settings file.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Load without writing; only absence of the file uses default settings.
    pub fn load(&self) -> Result<Config, ConfigError> {
        match File::open(&self.path) {
            Ok(file) => Ok(serde_json::from_reader(file)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(error) => Err(error.into()),
        }
    }

    /// Commit a patch against the revision last seen by the caller.
    pub fn update(
        &self,
        patch: &ConfigPatch,
        capabilities: &Capabilities,
    ) -> Result<Config, ConfigError> {
        let _lock = crate::storage::lock(&self.path.with_extension("json.lock"))?;
        let mut config = self.load()?;
        if config.revision != patch.expected_revision {
            return Err(ConfigError::RevisionConflict);
        }
        if let Some(backend) = &patch.backend {
            config.backend.clone_from(backend);
        }
        if let Some(volume) = patch.volume {
            config.volume = volume;
        }
        if let Some(device) = patch.tts_device {
            config.tts_device = device;
        }
        if let Some(device) = patch.alignment_device {
            config.alignment_device = device;
        }
        if let Some(enabled) = patch.alignment_enabled {
            config.alignment_enabled = enabled;
        }
        if let Some(speed) = patch.speed {
            config.speed = speed;
        }
        if let Some(voice) = &patch.voice {
            config.voice.clone_from(voice);
        }
        if let Some(auto_play) = patch.auto_play {
            config.auto_play = auto_play;
        }
        validate(&config, capabilities)?;
        config.revision = config
            .revision
            .checked_add(1)
            .ok_or_else(|| ConfigError::Invalid("revision overflow".into()))?;
        crate::storage::save(&self.path, &config)?;
        Ok(config)
    }
}

/// Validate settings against the actually available backend; never silently reset.
pub fn validate(config: &Config, capabilities: &Capabilities) -> Result<(), ConfigError> {
    if config.backend != capabilities.backend {
        return Err(ConfigError::Invalid(format!(
            "backend {} is unavailable",
            config.backend
        )));
    }
    if !config.volume.is_finite() || !(0.0..=10.0).contains(&config.volume) {
        return Err(ConfigError::Invalid("volume must be 0..10".into()));
    }
    if !config.speed.is_finite() || !(0.5..=2.0).contains(&config.speed) {
        return Err(ConfigError::Invalid("speed must be 0.5..2".into()));
    }
    if !capabilities.voices.contains(&config.voice) {
        return Err(ConfigError::Invalid(format!(
            "voice {} is unavailable",
            config.voice
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn capabilities() -> Capabilities {
        Capabilities {
            default_voice: "Zf001".into(),
            voice_names: Default::default(),
            backend: "kokoro".into(),
            voices: vec!["Zf001".into()],
            native_streaming: false,
            style: false,
            cloning: false,
            pronunciation: false,
        }
    }

    #[test]
    fn alignment_switch_persists_without_changing_voice_or_device() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("config.json"));
        let caps = capabilities();
        let current = store
            .update(
                &ConfigPatch {
                    backend: Some("kokoro".into()),
                    voice: Some("Zf001".into()),
                    alignment_enabled: Some(true),
                    ..Default::default()
                },
                &caps,
            )
            .unwrap();
        assert!(current.alignment_enabled);
        let changed = store
            .update(
                &ConfigPatch {
                    expected_revision: current.revision,
                    alignment_enabled: Some(false),
                    ..Default::default()
                },
                &caps,
            )
            .unwrap();
        assert!(!changed.alignment_enabled);
        assert_eq!(changed.voice, current.voice);
        assert_eq!(changed.alignment_device, current.alignment_device);
        assert_eq!(store.load().unwrap(), changed);
    }

    #[test]
    fn legacy_load_and_drop_do_not_write_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let original = include_str!("../../novel-tts-protocol/tests/fixtures/legacy-config.json");
        std::fs::write(&path, original).unwrap();
        let store = ConfigStore::new(&path);
        let config = store.load().unwrap();
        assert_eq!(config.voice, "Zm009");
        drop(config);
        drop(store);
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn commits_preserve_unknown_fields_and_reject_stale_revision() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        std::fs::write(
            &path,
            include_str!("../../novel-tts-protocol/tests/fixtures/legacy-config.json"),
        )
        .unwrap();
        let store = ConfigStore::new(path);
        let voices = Capabilities {
            default_voice: "Zf001".into(),
            voice_names: Default::default(),
            voices: vec!["Zm009".into()],
            backend: "kokoro".into(),
            native_streaming: false,
            style: false,
            cloning: false,
            pronunciation: false,
        };
        let patch = ConfigPatch {
            volume: Some(0.5),
            ..Default::default()
        };
        let config = store.update(&patch, &voices).unwrap();
        assert_eq!(config.extra["future_backend_settings"]["keep"], true);
        assert!(matches!(
            store.update(&patch, &voices),
            Err(ConfigError::RevisionConflict)
        ));
        assert_eq!(store.load().unwrap().volume, 0.5);
    }

    #[test]
    fn malformed_unavailable_and_invalid_settings_preserve_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let store = ConfigStore::new(&path);
        let voices = capabilities();
        std::fs::write(&path, b"broken").unwrap();
        assert!(store.update(&ConfigPatch::default(), &voices).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"broken");
        let config = Config {
            backend: "future".into(),
            ..Default::default()
        };
        crate::storage::save(&path, &config).unwrap();
        assert!(store.update(&ConfigPatch::default(), &voices).is_err());
        assert_eq!(store.load().unwrap().backend, "future");
        crate::storage::save(&path, &Config::default()).unwrap();
        assert!(
            store
                .update(
                    &ConfigPatch {
                        volume: Some(f32::NAN),
                        ..Default::default()
                    },
                    &voices
                )
                .is_err()
        );
        assert_eq!(store.load().unwrap().volume, 1.0);
    }

    #[test]
    fn held_transaction_lock_prevents_concurrent_writes() {
        let dir = tempfile::tempdir().unwrap();
        let store = ConfigStore::new(dir.path().join("config.json"));
        let _lock = crate::storage::lock(&store.path.with_extension("json.lock")).unwrap();
        assert!(
            store
                .update(&ConfigPatch::default(), &capabilities())
                .is_err()
        );
        assert!(!store.path.exists());
    }

    #[test]
    fn another_process_cannot_overwrite_a_locked_transaction() {
        if let Some(path) = std::env::var_os("TRNOVEL_CONFIG_LOCK_TEST") {
            let store = ConfigStore::new(PathBuf::from(path));
            assert!(
                store
                    .update(&ConfigPatch::default(), &capabilities())
                    .is_err()
            );
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let _lock = crate::storage::lock(&path.with_extension("json.lock")).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "config::tests::another_process_cannot_overwrite_a_locked_transaction",
            ])
            .env("TRNOVEL_CONFIG_LOCK_TEST", &path)
            .status()
            .unwrap();
        assert!(status.success());
        assert!(!path.exists());
    }
}
