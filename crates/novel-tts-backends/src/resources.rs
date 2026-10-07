//! Integrity and download policy shared by immutable model manifests.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{io::Read, path::Path};
use tts_protocol::Event;

#[derive(Deserialize)]
pub(crate) struct Resource {
    pub path: String,
    #[serde(default)]
    pub url: String,
    pub sha256: String,
    pub size: u64,
}

pub(crate) fn verify(path: &Path, resource: &Resource) -> anyhow::Result<()> {
    let mut file = std::fs::File::open(path)?;
    anyhow::ensure!(
        file.metadata()?.len() == resource.size,
        "invalid model size: {}",
        path.display()
    );
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    anyhow::ensure!(
        format!("{:x}", hash.finalize()) == resource.sha256,
        "model checksum mismatch: {}",
        path.display()
    );
    Ok(())
}

pub(crate) async fn prepare(
    directory: &Path,
    namespace: &str,
    resources: Vec<Resource>,
    progress: tokio::sync::mpsc::Sender<Event>,
) -> anyhow::Result<()> {
    for resource in resources {
        let path = directory.join(&resource.path);
        let name = format!("{namespace}/{}", resource.path);
        let tx = progress.clone();
        let download_name = name.clone();
        let size = resource.size;
        tts_core::download::download_from_url(&resource.url, &path, move |downloaded, _| {
            let _ = tx.try_send(Event::ModelProgress {
                resource: download_name.clone(),
                downloaded,
                total: size,
            });
        })
        .await?;
        let _ = progress
            .send(Event::ResourceState {
                stage: "校验".into(),
                resource: name,
            })
            .await;
        tokio::task::spawn_blocking(move || {
            if let Err(error) = verify(&path, &resource) {
                let _ = std::fs::rename(&path, path.with_extension("corrupt"));
                return Err(error);
            }
            Ok(())
        })
        .await??;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_corruption_and_wrong_size() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("model");
        let resource = Resource {
            path: "model".into(),
            url: String::new(),
            size: 3,
            sha256: format!("{:x}", Sha256::digest(b"abc")),
        };
        std::fs::write(&path, b"abc").unwrap();
        verify(&path, &resource).unwrap();
        for bad in [b"abd".as_slice(), b"a".as_slice()] {
            std::fs::write(&path, bad).unwrap();
            assert!(verify(&path, &resource).is_err());
        }
    }
}
