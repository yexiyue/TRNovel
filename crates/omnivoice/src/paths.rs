#[cfg(test)]
use std::path::PathBuf;

#[cfg(test)]
pub(crate) fn omnivoice_root() -> PathBuf {
    if let Some(root) = std::env::var_os("OMNIVOICE_ROOT") {
        return PathBuf::from(root);
    }

    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    if cwd.join("model").exists() || cwd.join("artifacts").exists() {
        return cwd;
    }

    let manifest_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    if manifest_root.join("Cargo.toml").exists() {
        return manifest_root;
    }

    cwd
}

#[cfg(test)]
pub(crate) fn ref_audio_path() -> PathBuf {
    omnivoice_root().join("ref.wav")
}
