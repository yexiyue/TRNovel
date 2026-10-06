use std::{fs::File, io::Write, path::Path};

/// Replace within the destination directory, so a failed write preserves the old file.
pub(crate) fn save<T: serde::Serialize>(path: &Path, value: &T) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("missing parent directory"))?;
    std::fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path).map_err(|error| error.error)?;
    // Unix directory fsync makes the rename durable; Windows cannot open a directory this way.
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}

/// Shared files are locked only during a read/modify/write transaction.
pub(crate) fn lock(path: &Path) -> std::io::Result<File> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::options()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path)?;
    file.try_lock().map_err(std::io::Error::other)?;
    Ok(file)
}
