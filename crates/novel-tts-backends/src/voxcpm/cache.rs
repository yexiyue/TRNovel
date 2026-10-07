//! Rebuild Candle caches from WAV; never interpret native features.json as Candle data.
use candle_core::{DType, Device, Tensor};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::Path;
use voxcpm::{Model, Reference};
const FILE: &str = "candle-reference-v1.json";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Identity {
    implementation: String,
    model: String,
    revision: String,
    weights: String,
    precision: String,
    wav: String,
    transcript: Option<String>,
}
#[derive(Serialize, Deserialize)]
struct Saved {
    identity: Identity,
    frames: usize,
    features: Vec<f32>,
}
impl Saved {
    fn valid(&self, identity: &Identity) -> bool {
        self.identity == *identity
            && (1..=188).contains(&self.frames)
            && self.features.len() == self.frames * 4 * 64
            && self.features.iter().all(|v| v.is_finite())
    }
}
/// Keep at most one encoded voice resident on the model device.
#[derive(Default)]
pub(super) struct Cache(Option<(Identity, Reference)>);
impl Cache {
    pub(super) fn reference(
        &mut self,
        model: &mut Model,
        variant: super::models::Model,
        device: &Device,
        directory: &Path,
        voice: &str,
        cancel: &impl Fn() -> bool,
    ) -> anyhow::Result<Reference> {
        let store = super::voice_store_for(directory, variant)?;
        let record = store.load(voice)?;
        let path = store.path(voice)?;
        let wav = path.join("reference.wav");
        let identity = Identity {
            implementation: "voxcpm-candle-0.11.0-v1".into(),
            model: variant.id().into(),
            revision: variant.revision().into(),
            weights: format!("{:x}", Sha256::digest(variant.manifest().as_bytes())),
            precision: format!("{:?}", variant.dtype(device)).to_lowercase(),
            wav: format!("{:x}", Sha256::digest(std::fs::read(&wav)?)),
            transcript: Some(record.transcript),
        };
        anyhow::ensure!(!cancel(), "reference encoding cancelled");
        if let Some((key, reference)) = &self.0
            && *key == identity
        {
            return Ok(Reference {
                features: reference.features.clone(),
                transcript: reference.transcript.clone(),
            });
        }
        self.0 = None;
        let cache = path.join(FILE);
        let saved = std::fs::metadata(&cache)
            .ok()
            .filter(|m| m.len() <= 2_000_000)
            .and_then(|_| std::fs::read(&cache).ok())
            .and_then(|bytes| serde_json::from_slice::<Saved>(&bytes).ok())
            .filter(|saved| saved.valid(&identity));
        let reference = if let Some(saved) = saved {
            Reference {
                features: Tensor::from_vec(saved.features, (saved.frames, 4, 64), device)?
                    .to_dtype(variant.dtype(device))?,
                transcript: identity.transcript.clone(),
            }
        } else {
            let audio = crate::reference::load(&wav, 16000)?;
            anyhow::ensure!(!cancel(), "reference encoding cancelled");
            let reference = model.reference(&audio, identity.transcript.clone(), cancel)?;
            anyhow::ensure!(!cancel(), "reference encoding cancelled");
            let saved = Saved {
                identity: identity.clone(),
                frames: reference.features.dim(0)?,
                features: reference
                    .features
                    .to_dtype(DType::F32)?
                    .flatten_all()?
                    .to_vec1()?,
            };
            anyhow::ensure!(saved.valid(&identity), "invalid Candle reference features");
            let temporary = tempfile::NamedTempFile::new_in(&path)?;
            serde_json::to_writer(temporary.as_file(), &saved)?;
            anyhow::ensure!(!cancel(), "reference encoding cancelled");
            temporary.persist(&cache)?;
            reference
        };
        anyhow::ensure!(!cancel(), "reference encoding cancelled");
        let result = Reference {
            features: reference.features.clone(),
            transcript: reference.transcript.clone(),
        };
        self.0 = Some((identity, reference));
        Ok(result)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn incompatible_and_corrupt_reference_caches_are_rebuilt() {
        let identity = Identity {
            implementation: "voxcpm-candle-v1".into(),
            model: "2b-q8_0".into(),
            revision: "revision".into(),
            weights: "manifest-sha".into(),
            precision: "f32".into(),
            wav: "wav-sha".into(),
            transcript: Some("参考文字".into()),
        };
        let mut saved = Saved {
            identity: identity.clone(),
            frames: 1,
            features: vec![0.; 256],
        };
        assert!(saved.valid(&identity));
        for field in 0..7 {
            let mut changed = identity.clone();
            match field {
                0 => changed.implementation.push('2'),
                1 => changed.model.push('2'),
                2 => changed.revision.push('2'),
                3 => changed.weights.push('2'),
                4 => changed.wav.push('2'),
                5 => changed.precision = "f16".into(),
                _ => changed.transcript = None,
            }
            assert!(!saved.valid(&changed));
        }
        saved.features[0] = f32::NAN;
        assert!(!saved.valid(&identity));
        saved.features[0] = 0.;
        saved.features.pop();
        assert!(!saved.valid(&identity));
        saved.frames = usize::MAX;
        assert!(!saved.valid(&identity));
        assert!(serde_json::from_str::<Saved>("[0, 1, 2]").is_err());
    }
}
