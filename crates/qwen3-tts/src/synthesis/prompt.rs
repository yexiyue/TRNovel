//! Portable encoded references; the adapter controls model identity and cache lifetime.
use super::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
struct EncodedPrompt {
    embedding: Vec<f32>,
    codes: Option<FrameCodes>,
    text_ids: Option<Vec<u32>>,
}
impl VoiceClonePrompt {
    pub fn save(&self, path: &Path) -> Result<()> {
        let embedding = self
            .speaker_embedding
            .to_dtype(DType::F32)?
            .flatten_all()?
            .to_vec1()?;
        let codes = self
            .ref_codes
            .as_ref()
            .map(|codes| codes.to_dtype(DType::U32)?.to_vec2::<u32>())
            .transpose()?;
        let file = std::fs::File::create(path)?;
        serde_json::to_writer(
            &file,
            &EncodedPrompt {
                embedding,
                codes,
                text_ids: self.ref_text_ids.clone(),
            },
        )?;
        file.sync_all()?;
        Ok(())
    }
    pub fn load(path: &Path, device: &Device) -> Result<Self> {
        let data: EncodedPrompt = serde_json::from_reader(std::fs::File::open(path)?)?;
        anyhow::ensure!(
            !data.embedding.is_empty() && data.embedding.iter().all(|v| v.is_finite()),
            "invalid speaker embedding cache"
        );
        let speaker_embedding = Tensor::new(data.embedding.as_slice(), device)?;
        let ref_codes = data
            .codes
            .map(|codes| -> Result<Tensor> {
                anyhow::ensure!(
                        !codes.is_empty()
                            && codes.len() <= 1200
                            && codes
                                .iter()
                                .all(|frame| frame.len() == 16
                                    && frame.iter().all(|code| *code < 2048)),
                        "invalid reference codec cache"
                    );
                let len = codes.len();
                Ok(Tensor::from_vec(
                    codes.into_iter().flatten().collect::<Vec<_>>(),
                    (len, 16),
                    device,
                )?
                .to_dtype(DType::I64)?)
            })
            .transpose()?;
        anyhow::ensure!(
            ref_codes.is_some() == data.text_ids.is_some(),
            "incomplete clone prompt cache"
        );
        Ok(Self {
            speaker_embedding,
            ref_codes,
            ref_text_ids: data.text_ids,
        })
    }
}
