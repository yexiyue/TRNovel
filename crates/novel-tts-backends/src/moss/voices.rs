//! Durable reference codes, independent of inference lifetime.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::File,
    path::{Path, PathBuf},
};

#[derive(Deserialize, Serialize)]
pub struct Voice {
    pub id: String,
    pub name: String,
    pub revision: String,
    pub codes: Vec<Vec<i32>>,
}
pub struct VoiceStore {
    directory: PathBuf,
}
impl VoiceStore {
    pub fn new(root: &Path) -> Self {
        Self {
            directory: root.join("voices"),
        }
    }
    fn path(&self, id: &str) -> anyhow::Result<PathBuf> {
        let name = id
            .strip_prefix("custom:")
            .ok_or_else(|| anyhow::anyhow!("custom voice IDs start with custom:"))?;
        anyhow::ensure!(
            !name.is_empty()
                && name.len() <= 64
                && name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_'),
            "voice ID must contain 1..64 ASCII letters, digits, '-' or '_'"
        );
        Ok(self.directory.join(format!("{name}.json")))
    }
    fn lock(&self) -> anyhow::Result<File> {
        std::fs::create_dir_all(&self.directory)?;
        let file = File::options()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.directory.join(".lock"))?;
        file.try_lock()?;
        Ok(file)
    }
    pub fn list(&self) -> anyhow::Result<Vec<Voice>> {
        if !self.directory.exists() {
            return Ok(Vec::new());
        }
        let mut voices = Vec::new();
        for item in std::fs::read_dir(&self.directory)? {
            let path = item?.path();
            if path.extension().is_some_and(|ext| ext == "json") {
                let voice: Voice = serde_json::from_reader(File::open(&path)?)?;
                anyhow::ensure!(
                    self.path(&voice.id)? == path,
                    "voice ID does not match cache filename"
                );
                anyhow::ensure!(
                    voice.revision == super::resources::REVISION,
                    "voice {} requires re-import for this model revision",
                    voice.id
                );
                validate_codes(&voice.codes)?;
                voices.push(voice);
            }
        }
        voices.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(voices)
    }
    pub fn save(&self, id: String, name: String, codes: Vec<Vec<i32>>) -> anyhow::Result<()> {
        let path = self.path(&id)?;
        anyhow::ensure!(
            !name.trim().is_empty() && name.len() <= 256,
            "voice name must be 1..256 bytes"
        );
        validate_codes(&codes)?;
        let _lock = self.lock()?;
        let mut file = tempfile::NamedTempFile::new_in(&self.directory)?;
        serde_json::to_writer(
            &mut file,
            &Voice {
                id,
                name,
                revision: super::resources::REVISION.into(),
                codes,
            },
        )?;
        file.as_file().sync_all()?;
        file.persist_noclobber(path)?;
        Ok(())
    }
    pub fn remove(&self, id: &str) -> anyhow::Result<()> {
        let path = self.path(id)?;
        let _lock = self.lock()?;
        std::fs::remove_file(path)?;
        Ok(())
    }
    pub(super) fn codes(&self, id: &str, manifest: &Value) -> anyhow::Result<Vec<Vec<i32>>> {
        if id.starts_with("custom:") {
            let voice: Voice = serde_json::from_reader(File::open(self.path(id)?)?)?;
            anyhow::ensure!(
                voice.id == id && voice.revision == super::resources::REVISION,
                "voice cache identity/revision mismatch; re-import {id}"
            );
            validate_codes(&voice.codes)?;
            return Ok(voice.codes);
        }
        let voice = manifest["builtin_voices"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["voice"] == id)
            .ok_or_else(|| anyhow::anyhow!("unknown MOSS voice {id}"))?;
        let codes: Vec<Vec<i32>> = serde_json::from_value(voice["prompt_audio_codes"].clone())?;
        validate_codes(&codes)?;
        Ok(codes)
    }
}
fn validate_codes(codes: &[Vec<i32>]) -> anyhow::Result<()> {
    anyhow::ensure!(
        !codes.is_empty()
            && codes.len() <= 375
            && codes
                .iter()
                .all(|row| row.len() == 16 && row.iter().all(|v| (0..1024).contains(v))),
        "invalid reference audio codes"
    );
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn atomic_voice_cache_rejects_overwrite_traversal_and_stale_revision() {
        let directory = tempfile::tempdir().unwrap();
        let store = VoiceStore::new(directory.path());
        let codes = vec![vec![1; 16]; 20];
        store
            .save("custom:reader".into(), "Reader".into(), codes.clone())
            .unwrap();
        assert!(
            store
                .save("custom:reader".into(), "Other".into(), codes.clone())
                .is_err()
        );
        assert!(
            store
                .save("custom:../escape".into(), "Other".into(), codes)
                .is_err()
        );
        assert_eq!(store.list().unwrap()[0].name, "Reader");
        assert!(store.remove("Weiguo").is_err());
        let path = store.path("custom:reader").unwrap();
        let mut voice: Voice = serde_json::from_reader(File::open(&path).unwrap()).unwrap();
        voice.revision = "old".into();
        serde_json::to_writer(File::create(path).unwrap(), &voice).unwrap();
        assert!(store.list().is_err());
        store.remove("custom:reader").unwrap();
        assert!(store.list().unwrap().is_empty());
    }
}
