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
    defaults: Config,
}

impl ConfigStore {
    /// Use an explicit path, including an isolated path in tests.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            defaults: Config::default(),
        }
    }

    /// Application-selected defaults apply only when no settings file exists.
    pub fn with_defaults(mut self, defaults: Config) -> Self {
        self.defaults = defaults;
        self
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
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(self.defaults.clone()),
            Err(error) => Err(error.into()),
        }
    }

    /// Retire the former CPU backends while preserving unrelated preferences.
    pub fn migrate_retired_backend(&self, nano: &Capabilities) -> Result<(), ConfigError> {
        if !matches!(self.load()?.backend.as_str(), "kokoro" | "zipvoice") {
            return Ok(());
        }
        let _lock = crate::storage::lock(&self.path.with_extension("json.lock"))?;
        let mut config = self.load()?;
        if !matches!(config.backend.as_str(), "kokoro" | "zipvoice") {
            return Ok(());
        }
        if nano.backend != "moss" || nano.model.as_deref().is_some_and(|id| id != "nano") {
            return Err(ConfigError::Invalid("migration requires MOSS Nano".into()));
        }
        config.backend.clone_from(&nano.backend);
        config.model.clone_from(&nano.model);
        config.voice.clone_from(&nano.default_voice);
        config.style = None;
        config.tts_device = tts_protocol::Device::Cpu;
        validate(&config, nano)?;
        config.revision = config
            .revision
            .checked_add(1)
            .ok_or_else(|| ConfigError::Invalid("revision overflow".into()))?;
        crate::storage::save(&self.path, &config)?;
        Ok(())
    }

    /// Persist the selected defaults on first activation, without rewriting old files.
    pub fn initialize(&self, capabilities: &Capabilities) -> Result<Config, ConfigError> {
        let _lock = crate::storage::lock(&self.path.with_extension("json.lock"))?;
        let config = self.load()?;
        validate(&config, capabilities)?;
        if !self.path.exists() {
            crate::storage::save(&self.path, &config)?;
        }
        Ok(config)
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
        config.model = patch.target_model(&config).map(str::to_owned);
        if let Some(style) = &patch.style {
            config.style = (!style.trim().is_empty()).then(|| style.clone());
        } else if !capabilities.style {
            config.style = None;
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
    if !capabilities.matches(&config.backend, config.model.as_deref()) {
        return Err(ConfigError::Invalid(format!(
            "backend {} / model {:?} is unavailable",
            config.backend, config.model
        )));
    }
    if config.style.as_ref().is_some_and(|style| {
        !capabilities.style || style.trim().is_empty() || style.chars().count() > 200
    }) {
        return Err(ConfigError::Invalid(
            "style is unsupported by this model or exceeds 200 characters".into(),
        ));
    }
    if !config.volume.is_finite() || !(0.0..=10.0).contains(&config.volume) {
        return Err(ConfigError::Invalid("volume must be 0..10".into()));
    }
    if !config.speed.is_finite() || !(0.5..=2.0).contains(&config.speed) {
        return Err(ConfigError::Invalid("speed must be 0.5..2".into()));
    }
    if !capabilities.voices.contains(&config.voice) {
        if capabilities.cloning && capabilities.voices.is_empty() {
            return Err(ConfigError::Invalid("this model requires a saved reference voice; import or design a voice before selecting it".into()));
        }
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

    #[test]
    fn retired_backends_migrate_once_and_preserve_preferences() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let store = ConfigStore::new(&path);
        let mut nano = capabilities();
        nano.backend = "moss".into();
        nano.model = Some("nano".into());
        nano.default_voice = "Weiguo".into();
        nano.voices = vec!["Weiguo".into()];
        for backend in [Some("kokoro"), Some("zipvoice"), None] {
            let mut old = serde_json::json!({
                "voice": "custom:old", "model": "distill-fp32", "style": "old",
                "volume": 0.5, "speed": 1.4, "auto_play": true, "revision": 8,
                "tts_device": "cuda", "alignment_device": "coreml",
                "alignment_enabled": true, "future": {"keep": true}
            });
            if let Some(backend) = backend {
                old["backend"] = backend.into();
            }
            std::fs::write(&path, serde_json::to_vec(&old).unwrap()).unwrap();
            store.migrate_retired_backend(&nano).unwrap();
            let migrated = store.load().unwrap();
            assert_eq!(migrated.backend, "moss");
            assert_eq!(migrated.model.as_deref(), Some("nano"));
            assert_eq!(migrated.voice, "Weiguo");
            assert_eq!(migrated.tts_device, tts_protocol::Device::Cpu);
            assert_eq!(migrated.style, None);
            assert_eq!(migrated.volume, 0.5);
            assert_eq!(migrated.speed, 1.4);
            assert!(migrated.auto_play && migrated.alignment_enabled);
            assert_eq!(migrated.alignment_device, tts_protocol::Device::Coreml);
            assert_eq!(migrated.extra["future"]["keep"], true);
            assert_eq!(migrated.revision, 9);
            let bytes = std::fs::read(&path).unwrap();
            store.migrate_retired_backend(&nano).unwrap();
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
    }

    #[test]
    fn failed_migration_and_other_backends_preserve_existing_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let store = ConfigStore::new(&path);
        for backend in ["qwen", "future", "kokoro"] {
            let bytes = serde_json::to_vec(&Config {
                backend: backend.into(),
                ..Default::default()
            })
            .unwrap();
            std::fs::write(&path, &bytes).unwrap();
            assert_eq!(
                store.migrate_retired_backend(&capabilities()).is_err(),
                backend == "kokoro"
            );
            assert_eq!(std::fs::read(&path).unwrap(), bytes);
        }
    }

    fn capabilities() -> Capabilities {
        Capabilities {
            model: None,
            model_name: String::new(),
            default_voice: "Zf001".into(),
            voice_names: Default::default(),
            backend: "kokoro".into(),
            voices: vec!["Zf001".into()],
            native_streaming: false,
            style: false,
            compiled_devices: Vec::new(),
            cloning: false,
            pronunciation: false,
        }
    }

    #[test]
    fn activation_persists_defaults_once_and_preserves_existing_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let defaults = Config {
            backend: "kokoro".into(),
            voice: "Zf001".into(),
            tts_device: tts_protocol::Device::Cpu,
            ..Default::default()
        };
        let store = ConfigStore::new(&path).with_defaults(defaults.clone());
        assert!(!path.exists());
        assert_eq!(store.initialize(&capabilities()).unwrap(), defaults);
        let reopened = ConfigStore::new(&path);
        assert_eq!(reopened.load().unwrap(), defaults);
        let original = "{\n  \"backend\":\"kokoro\",\"voice\":\"Zf001\",\"tts_device\":\"cpu\",\"future\":true\n}";
        std::fs::write(&path, original).unwrap();
        reopened.initialize(&capabilities()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        let absent = ConfigStore::new(directory.path().join("invalid.json"));
        assert!(absent.initialize(&capabilities()).is_err());
        assert!(!absent.path().exists());
    }

    #[test]
    fn new_model_defaults_preserve_existing_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let defaults = Config {
            backend: "qwen".into(),
            model: Some("1.7b-customvoice".into()),
            voice: "uncle_fu".into(),
            ..Config::default()
        };
        let store = ConfigStore::new(&path).with_defaults(defaults.clone());
        assert_eq!(store.load().unwrap(), defaults);
        assert!(!path.exists());
        let original = r#"{"backend":"qwen","voice":"serena","tts_device":"cpu"}"#;
        std::fs::write(&path, original).unwrap();
        let existing = store.load().unwrap();
        assert_eq!(existing.model, None);
        assert_eq!(existing.voice, "serena");
        assert_eq!(existing.tts_device, tts_protocol::Device::Cpu);
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    }
    #[test]
    fn speaking_style_is_validated_and_cleared_on_an_unsupported_model() {
        let dir = tempfile::tempdir().unwrap();
        let mut caps = capabilities();
        caps.style = true;
        let store = ConfigStore::new(dir.path().join("config.json")).with_defaults(Config {
            backend: "kokoro".into(),
            voice: "Zf001".into(),
            ..Default::default()
        });
        let changed = store
            .update(
                &ConfigPatch {
                    style: Some("温暖沉稳".into()),
                    ..Default::default()
                },
                &caps,
            )
            .unwrap();
        assert_eq!(changed.style.as_deref(), Some("温暖沉稳"));
        caps.style = false;
        assert!(validate(&changed, &caps).is_err());
        assert!(
            store
                .update(
                    &ConfigPatch {
                        expected_revision: changed.revision,
                        ..Default::default()
                    },
                    &caps
                )
                .unwrap()
                .style
                .is_none()
        );
    }

    #[test]
    fn model_identity_is_validated_and_cleared_when_backend_changes() {
        let dir = tempfile::tempdir().unwrap();
        let defaults = Config {
            backend: "qwen".into(),
            model: Some("1.7b-customvoice".into()),
            voice: "uncle_fu".into(),
            ..Config::default()
        };
        let store =
            ConfigStore::new(dir.path().join("config.json")).with_defaults(defaults.clone());
        let mut qwen = capabilities();
        qwen.backend = "qwen".into();
        qwen.model = Some("0.6b-customvoice".into());
        qwen.voices = vec!["uncle_fu".into()];
        assert!(validate(&defaults, &qwen).is_err());
        let changed = store
            .update(
                &ConfigPatch {
                    backend: Some("kokoro".into()),
                    voice: Some("Zf001".into()),
                    ..Default::default()
                },
                &capabilities(),
            )
            .unwrap();
        assert_eq!(changed.model, None);
        assert_eq!(changed.backend, "kokoro");
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
            model: None,
            model_name: String::new(),
            default_voice: "Zf001".into(),
            voice_names: Default::default(),
            voices: vec!["Zm009".into()],
            backend: "kokoro".into(),
            native_streaming: false,
            style: false,
            compiled_devices: Vec::new(),
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
