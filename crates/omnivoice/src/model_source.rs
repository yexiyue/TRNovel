use crate::{
    artifacts::RUNTIME_MANIFEST_FILE_NAME,
    error::{OmniVoiceError, Result},
};
use std::path::{Path, PathBuf};
pub(crate) fn ensure_local_runtime_manifest(model_root: &Path) -> Result<PathBuf> {
    let path = model_root.join(RUNTIME_MANIFEST_FILE_NAME);
    if !path.is_file() {
        return Err(OmniVoiceError::MissingArtifact { path });
    }
    Ok(path)
}
