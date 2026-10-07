//! Pinned Qwen model identities; the legacy directory remains valid.
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model {
    Custom06,
    Custom17,
    Base17,
    Design17,
}

impl Model {
    pub fn parse(id: Option<&str>) -> anyhow::Result<Self> {
        match id {
            None | Some("0.6b-customvoice") => Ok(Self::Custom06),
            Some("1.7b-customvoice") => Ok(Self::Custom17),
            Some("1.7b-base") => Ok(Self::Base17),
            Some("1.7b-voicedesign") => Ok(Self::Design17),
            Some(id) => anyhow::bail!("unknown Qwen model {id}"),
        }
    }
    pub fn id(self) -> &'static str {
        match self {
            Self::Custom06 => "0.6b-customvoice",
            Self::Custom17 => "1.7b-customvoice",
            Self::Base17 => "1.7b-base",
            Self::Design17 => "1.7b-voicedesign",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::Custom06 => "Qwen3 0.6B · 固定音色",
            Self::Custom17 => "Qwen3 1.7B · 固定音色",
            Self::Base17 => "Qwen3 1.7B · 克隆音色",
            Self::Design17 => "Qwen3 1.7B · 音色设计",
        }
    }
    pub fn revision(self) -> &'static str {
        match self {
            Self::Custom06 => super::resources::REVISION,
            Self::Custom17 => "0c0e3051f131929182e2c023b9537f8b1c68adfe",
            Self::Base17 => "fd4b254389122332181a7c3db7f27e918eec64e3",
            Self::Design17 => "5ecdb67327fd37bb2e042aab12ff7391903235d3",
        }
    }
    pub fn directory(self, root: &Path) -> PathBuf {
        if self == Self::Custom06 {
            root.join("qwen")
        } else {
            root.join("qwen/models")
                .join(self.id())
                .join(self.revision())
        }
    }
    pub(super) fn manifest(self) -> &'static str {
        match self {
            Self::Custom06 => include_str!("resources.json"),
            Self::Custom17 => include_str!("models/1.7b-customvoice.json"),
            Self::Base17 => include_str!("models/1.7b-base.json"),
            Self::Design17 => include_str!("models/1.7b-voicedesign.json"),
        }
    }
    pub(super) fn detect(directory: &Path) -> anyhow::Result<Self> {
        let config: serde_json::Value =
            serde_json::from_reader(std::fs::File::open(directory.join("config.json"))?)?;
        match (
            config["tts_model_size"].as_str(),
            config["tts_model_type"].as_str(),
        ) {
            (Some("0b6"), Some("custom_voice")) => Ok(Self::Custom06),
            (Some("1b7"), Some("custom_voice")) => Ok(Self::Custom17),
            (Some("1b7"), Some("base")) => Ok(Self::Base17),
            (Some("1b7"), Some("voice_design")) => Ok(Self::Design17),
            other => anyhow::bail!("unsupported Qwen model metadata {other:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_and_explicit_models_have_isolated_resources() {
        let root = Path::new("cache");
        assert_eq!(
            Model::parse(None).unwrap().directory(root),
            root.join("qwen")
        );
        assert_ne!(
            Model::Custom17.directory(root),
            Model::Base17.directory(root)
        );
        assert!(Model::parse(Some("../model")).is_err());
        for model in [
            Model::Custom06,
            Model::Custom17,
            Model::Base17,
            Model::Design17,
        ] {
            let resources: Vec<crate::resources::Resource> =
                serde_json::from_str(model.manifest()).unwrap();
            assert!(
                resources.iter().all(|r| r.url.contains(model.revision())
                    && r.sha256.len() == 64
                    && r.size > 0)
            );
        }
    }
}
