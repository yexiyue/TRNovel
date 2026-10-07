//! Model-scoped reference recordings. Adapter encoding caches live beside the reference.
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub backend: String,
    pub model: String,
    pub revision: String,
    pub transcript: String,
    pub description: Option<String>,
}

pub struct VoiceStore {
    directory: PathBuf,
    backend: String,
    model: String,
    revision: String,
}
impl VoiceStore {
    pub fn new(root: &Path, backend: &str, model: &str, revision: &str) -> anyhow::Result<Self> {
        for component in [backend, model, revision] {
            anyhow::ensure!(
                !component.is_empty()
                    && component.len() <= 128
                    && component
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
                    && component != "."
                    && component != "..",
                "invalid model identity"
            );
        }
        Ok(Self {
            directory: root.join("voices").join(backend).join(model).join(revision),
            backend: backend.into(),
            model: model.into(),
            revision: revision.into(),
        })
    }
    pub fn path(&self, id: &str) -> anyhow::Result<PathBuf> {
        let id = id
            .strip_prefix("custom:")
            .ok_or_else(|| anyhow::anyhow!("voice IDs must start with custom:"))?;
        anyhow::ensure!(
            !id.is_empty()
                && id.len() <= 64
                && id
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_')),
            "invalid voice ID: use 1..64 ASCII letters, digits, '-' or '_'"
        );
        Ok(self.directory.join(format!("voice_{id}")))
    }
    pub fn load(&self, id: &str) -> anyhow::Result<Voice> {
        let voice: Voice = serde_json::from_reader(File::open(self.path(id)?.join("voice.json"))?)?;
        anyhow::ensure!(
            voice.id == id
                && voice.backend == self.backend
                && voice.model == self.model
                && voice.revision == self.revision,
            "voice/model identity mismatch; re-import {id}"
        );
        anyhow::ensure!(
            !voice.transcript.trim().is_empty(),
            "reference transcript is missing"
        );
        Ok(voice)
    }
    pub fn list(&self) -> anyhow::Result<Vec<Voice>> {
        if !self.directory.exists() {
            return Ok(Vec::new());
        }
        let mut voices = Vec::new();
        for entry in std::fs::read_dir(&self.directory)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() || !entry.path().join("voice.json").exists() {
                continue;
            }
            let voice: Voice =
                serde_json::from_reader(File::open(entry.path().join("voice.json"))?)?;
            anyhow::ensure!(
                self.path(&voice.id)? == entry.path(),
                "voice filename identity mismatch"
            );
            voices.push(self.load(&voice.id)?);
        }
        voices.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(voices)
    }
    pub fn import(
        &self,
        id: &str,
        name: &str,
        wav: &Path,
        transcript: &str,
        description: Option<String>,
    ) -> anyhow::Result<()> {
        let destination = self.path(id)?;
        anyhow::ensure!(
            !name.trim().is_empty() && name.len() <= 256,
            "voice name must contain 1..256 bytes"
        );
        anyhow::ensure!(
            !transcript.trim().is_empty() && transcript.len() <= 16384,
            "provide the reference recording's transcript (up to 16384 bytes)"
        );
        let _lock = crate::storage::lock(&self.directory.join(".lock"))?;
        anyhow::ensure!(
            !destination.exists(),
            "voice {id} already exists; choose another ID or remove it first"
        );
        let staging = tempfile::tempdir_in(&self.directory)?;
        std::fs::copy(wav, staging.path().join("reference.wav"))?;
        let voice = Voice {
            id: id.into(),
            name: name.into(),
            backend: self.backend.clone(),
            model: self.model.clone(),
            revision: self.revision.clone(),
            transcript: transcript.into(),
            description,
        };
        crate::storage::save(&staging.path().join("voice.json"), &voice)?;
        std::fs::rename(staging.path(), destination)?;
        Ok(())
    }
    pub fn remove(&self, id: &str) -> anyhow::Result<()> {
        let path = self.path(id)?;
        let _lock = crate::storage::lock(&self.directory.join(".lock"))?;
        self.load(id)?;
        std::fs::remove_dir_all(path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_and_revision_isolate_voices_and_reject_traversal() {
        let root = tempfile::tempdir().unwrap();
        let store = VoiceStore::new(root.path(), "qwen", "1.7b-base", "rev1").unwrap();
        let wav = root.path().join("recording.wav");
        std::fs::write(&wav, b"recording").unwrap();
        assert!(
            store
                .import("custom:reader", "Reader", &wav, "", None)
                .is_err()
        );
        store
            .import("custom:reader", "Reader", &wav, "你好", None)
            .unwrap();
        assert_eq!(store.list().unwrap().len(), 1);
        assert!(
            VoiceStore::new(root.path(), "qwen", "1.7b-base", "rev2")
                .unwrap()
                .list()
                .unwrap()
                .is_empty()
        );
        assert!(
            VoiceStore::new(root.path(), "qwen", "0.6b-base", "rev1")
                .unwrap()
                .list()
                .unwrap()
                .is_empty()
        );
        assert!(store.path("custom:../reader").is_err());
        assert!(
            store
                .import("custom:reader", "Reader", &wav, "你好", None)
                .is_err()
        );
        store.remove("custom:reader").unwrap();
        assert!(store.list().unwrap().is_empty());
    }
}
