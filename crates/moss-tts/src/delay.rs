//! Parallel codebook heads with the official delay-pattern drain schedule.
use crate::{
    Generation,
    config::SpeechConfig,
    sampling::{Parameters, sample},
    transformer::Transformer,
};
use candle_core::{DType, Device, Module, Tensor};
use candle_nn::{Embedding, Linear, embedding, linear_no_bias};
use rand::{SeedableRng, rngs::StdRng};
use std::path::Path;
use tokenizers::Tokenizer;

pub struct DelayModel {
    pub config: SpeechConfig,
    pub tokenizer: Tokenizer,
    embeddings: Vec<Embedding>,
    backbone: Transformer,
    heads: Vec<Linear>,
    device: Device,
}
impl DelayModel {
    pub fn load(directory: &Path, device: &Device, dtype: DType) -> anyhow::Result<Self> {
        let config: SpeechConfig =
            serde_json::from_slice(&std::fs::read(directory.join("config.json"))?)?;
        config.validate()?;
        let vb = crate::weights(directory, dtype, device)?;
        let d = config.language_config.hidden_size;
        let mut embeddings = vec![embedding(
            config.language_config.vocab_size,
            d,
            vb.pp("language_model.embed_tokens"),
        )?];
        for i in 0..config.n_vq {
            embeddings.push(embedding(
                config.audio_vocab_size + 1,
                d,
                vb.pp(format!("emb_ext.{i}")),
            )?);
        }
        let heads = (0..=config.n_vq)
            .map(|i| {
                linear_no_bias(
                    d,
                    if i == 0 {
                        config.language_config.vocab_size
                    } else {
                        config.audio_vocab_size + 1
                    },
                    vb.pp(format!("lm_heads.{i}")),
                )
            })
            .collect::<candle_core::Result<_>>()?;
        let backbone = Transformer::load(&config.language_config, vb.pp("language_model"), true)?;
        let tokenizer = Tokenizer::from_file(directory.join("tokenizer.json"))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        Ok(Self {
            config,
            tokenizer,
            embeddings,
            backbone,
            heads,
            device: device.clone(),
        })
    }
    fn embed(&self, rows: &[Vec<u32>]) -> anyhow::Result<Tensor> {
        let mut result = None;
        for (i, embedding) in self.embeddings.iter().enumerate() {
            let ids = Tensor::from_vec(
                rows.iter().map(|row| row[i]).collect::<Vec<_>>(),
                (1, rows.len()),
                &self.device,
            )?;
            let value = embedding.forward(&ids)?;
            result = Some(match result {
                None => value,
                Some(previous) => (previous + value)?,
            });
        }
        Ok(result.expect("validated codebooks"))
    }
    pub fn generate(
        &mut self,
        request: &Generation<'_>,
        cancelled: &impl Fn() -> bool,
        mut frame: impl FnMut(&[u32]) -> anyhow::Result<bool>,
    ) -> anyhow::Result<()> {
        // Reference delay patterns differ from Local; never reuse Local encoded rows.
        anyhow::ensure!(
            request.reference.is_none(),
            "delay reference scheduling has not been verified"
        );
        self.backbone.reset();
        let prompt = crate::prompt::text_prompt(
            &self.tokenizer,
            &self.config,
            request.text,
            request.instruction,
            None,
        )?;
        let mut hidden = self.backbone.forward(&self.embed(&prompt)?, cancelled)?;
        let mut history: Vec<Vec<u32>> = (0..=self.config.n_vq)
            .map(|i| prompt.iter().map(|r| r[i]).collect())
            .collect();
        let mut generated: Vec<Vec<u32>> = Vec::new();
        let mut rng = StdRng::seed_from_u64(request.seed);
        let mut drain = None;
        let delay = 151662;
        let mut frames = 0;
        for step in 0..request.max_frames + self.config.n_vq + 1 {
            crate::check_cancel(cancelled)?;
            let last = hidden.narrow(1, hidden.dim(1)? - 1, 1)?;
            if drain == Some(self.config.n_vq) {
                return Ok(());
            }
            let token = if drain.is_some() {
                delay
            } else {
                let logits = self.heads[0].forward(&last)?.flatten_all()?;
                let mut values = logits.to_dtype(DType::F32)?.to_vec1::<f32>()?;
                for (id, value) in values.iter_mut().enumerate() {
                    if id != self.config.audio_assistant_gen_slot_token_id as usize
                        && (id != delay as usize || step == 0)
                    {
                        *value = f32::NEG_INFINITY;
                    }
                }
                sample(
                    &Tensor::from_vec(
                        values,
                        self.config.language_config.vocab_size,
                        &self.device,
                    )?,
                    &history[0],
                    Parameters::TEXT,
                    None,
                    &mut rng,
                )?
            };
            let mut row = vec![token];
            for i in 0..self.config.n_vq {
                let active = step + 1 > i && drain.is_none_or(|length| i >= length);
                let id = if active {
                    sample(
                        &self.heads[i + 1].forward(&last)?,
                        &history[i + 1],
                        Parameters {
                            temperature: 1.7,
                            top_k: 25,
                            top_p: 0.8,
                            penalty: 1.,
                        },
                        Some(self.config.audio_pad_code),
                        &mut rng,
                    )?
                } else {
                    self.config.audio_pad_code
                };
                row.push(id);
            }
            generated.push(row.clone());
            if step + 1 >= self.config.n_vq {
                let source = step + 1 - self.config.n_vq;
                let complete: Vec<_> = (0..self.config.n_vq)
                    .map(|i| generated[source + i][i + 1])
                    .collect();
                anyhow::ensure!(
                    complete.iter().all(|id| *id < self.config.audio_pad_code),
                    "invalid delay drain frame"
                );
                if !frame(&complete)? {
                    anyhow::bail!("MOSS inference cancelled");
                }
                frames += 1;
                anyhow::ensure!(
                    frames <= request.max_frames,
                    "MOSS frame limit reached before EOS"
                );
            }
            if token == delay {
                drain = Some(drain.map_or(1, |length| length + 1));
            }
            for (h, id) in history.iter_mut().zip(&row) {
                h.push(*id);
            }
            hidden = self.backbone.forward(&self.embed(&[row])?, cancelled)?;
        }
        anyhow::bail!("MOSS frame limit reached before EOS")
    }
}
