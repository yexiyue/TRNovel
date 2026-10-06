//! Independent, versioned listening checkpoints, never reader progress.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use tts_protocol::{SourceId, text_hash};

/// A last durable playback boundary for one immutable source snapshot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub schema_version: u32,
    pub source: SourceId,
    pub text_hash: String,
    pub resume_byte: usize,
    pub completed: bool,
    pub updated_at: u64,
}

/// Invalid checkpoints are reported rather than silently moved to another text.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    #[error("checkpoint IO failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid checkpoint JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("checkpoint invalidated: {0}")]
    Invalidated(&'static str),
}

/// Separate files per source make unrelated CLI/reader positions independent.
#[derive(Debug, Clone)]
pub struct CheckpointStore {
    directory: PathBuf,
}

impl CheckpointStore {
    /// Explicit directory for production or isolated tests.
    pub fn new(directory: impl Into<PathBuf>) -> Self {
        Self {
            directory: directory.into(),
        }
    }

    /// Listening data directory under the existing application home.
    pub fn user_default() -> Result<Self, CheckpointError> {
        let home =
            dirs::home_dir().ok_or(CheckpointError::Invalidated("home directory unavailable"))?;
        Ok(Self::new(home.join(".novel/tts/checkpoints")))
    }

    fn path(&self, source: &SourceId) -> Result<PathBuf, CheckpointError> {
        let identity = serde_json::to_string(source)?;
        Ok(self
            .directory
            .join(format!("{}.json", text_hash(&identity))))
    }

    /// Read only; a caller decides whether to initiate playback.
    pub fn load(
        &self,
        source: &SourceId,
        text: &str,
    ) -> Result<Option<Checkpoint>, CheckpointError> {
        let path = self.path(source)?;
        let file = match std::fs::File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        let checkpoint: Checkpoint = serde_json::from_reader(file)?;
        if checkpoint.schema_version != 1 {
            return Err(CheckpointError::Invalidated("unknown schema major version"));
        }
        if checkpoint.source != *source || checkpoint.text_hash != text_hash(text) {
            return Err(CheckpointError::Invalidated("source or text changed"));
        }
        if checkpoint.resume_byte > text.len() || !text.is_char_boundary(checkpoint.resume_byte) {
            return Err(CheckpointError::Invalidated("invalid UTF-8 position"));
        }
        Ok(Some(checkpoint))
    }

    /// Save the original-text playback boundary atomically.
    pub fn save(
        &self,
        source: &SourceId,
        text: &str,
        byte: usize,
        completed: bool,
    ) -> Result<(), CheckpointError> {
        if byte > text.len() || !text.is_char_boundary(byte) {
            return Err(CheckpointError::Invalidated("invalid UTF-8 position"));
        }
        let path = self.path(source)?;
        let _lock = crate::storage::lock(&path.with_extension("json.lock"))?;
        let checkpoint = Checkpoint {
            schema_version: 1,
            source: source.clone(),
            text_hash: text_hash(text),
            resume_byte: byte,
            completed,
            updated_at: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_err(std::io::Error::other)?
                .as_secs(),
        };
        crate::storage::save(&path, &checkpoint)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn source(namespace: &str) -> SourceId {
        SourceId {
            namespace: namespace.into(),
            book: "书".into(),
            chapter: "1".into(),
        }
    }

    #[test]
    fn unicode_snapshot_is_preserved_and_sources_are_independent() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path());
        let text = "中文🙂\r\n下一句。";
        store.save(&source("reader"), text, 6, false).unwrap();
        store.save(&source("cli"), text, 0, false).unwrap();
        assert_eq!(
            store
                .load(&source("reader"), text)
                .unwrap()
                .unwrap()
                .resume_byte,
            6
        );
        assert_eq!(
            store
                .load(&source("cli"), text)
                .unwrap()
                .unwrap()
                .resume_byte,
            0
        );
        assert!(store.load(&source("reader"), "正文变化").is_err());
        assert!(store.save(&source("reader"), text, 7, false).is_err());
    }

    #[test]
    fn unknown_versions_and_out_of_range_positions_do_not_restore() {
        let dir = tempfile::tempdir().unwrap();
        let store = CheckpointStore::new(dir.path());
        let id = source("reader");
        store.save(&id, "中", 0, false).unwrap();
        let path = store.path(&id).unwrap();
        let mut cp = store.load(&id, "中").unwrap().unwrap();
        cp.schema_version = 2;
        crate::storage::save(&path, &cp).unwrap();
        assert!(store.load(&id, "中").is_err());
        cp.schema_version = 1;
        cp.resume_byte = 4;
        crate::storage::save(&path, &cp).unwrap();
        assert!(store.load(&id, "中").is_err());
    }
}
