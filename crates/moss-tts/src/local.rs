//! Time-synchronous MOSS Local generation; each frame owns a fresh depth cache.
use crate::{
    config::LocalConfig,
    transformer::{Mlp, Transformer},
};
use candle_core::{DType, Device, Module, Tensor};
use candle_nn::{Embedding, Linear, RmsNorm, embedding, linear_no_bias, rms_norm};
use rand::{SeedableRng, rngs::StdRng};
use std::path::Path;
use tokenizers::Tokenizer;

pub struct LocalModel {
    pub config: LocalConfig,
    pub tokenizer: Tokenizer,
    embeddings: Vec<Embedding>,
    global: Transformer,
    depth: Transformer,
    to_depth: Mlp,
    from_depth: Vec<Mlp>,
    norms: Vec<RmsNorm>,
    heads: Vec<Linear>,
    device: Device,
}
impl LocalModel {
    pub fn load(directory: &Path, device: &Device, dtype: DType) -> anyhow::Result<Self> {
        let config: LocalConfig =
            serde_json::from_slice(&std::fs::read(directory.join("config.json"))?)?;
        config.validate()?;
        let tokenizer = Tokenizer::from_file(directory.join("tokenizer.json"))
            .map_err(|e| anyhow::anyhow!(e.to_string()))?;
        let vb = crate::weights(directory, dtype, device)?;
        let d = config.language_config.hidden_size;
        let embeddings = (0..=config.n_vq)
            .map(|i| {
                embedding(
                    if i == 0 {
                        config.language_config.vocab_size
                    } else {
                        config.audio_vocab_size + 1
                    },
                    d,
                    vb.pp(format!("model.embedding_list.{i}")),
                )
            })
            .collect::<candle_core::Result<_>>()?;
        let global =
            Transformer::load(&config.language_config, vb.pp("model.language_model"), true)?;
        let depth = Transformer::load(&config.depth_config(), vb.pp("local_transformer"), false)?;
        let to_depth = Mlp::load(
            d,
            config.additional_mlp_ffn_hidden_size,
            config.local_hidden_size,
            vb.pp("speech_embedding_to_local_mlp"),
        )?;
        let from_depth = (0..=config.n_vq)
            .map(|i| {
                Mlp::load(
                    config.local_hidden_size,
                    config.additional_mlp_ffn_hidden_size,
                    d,
                    vb.pp(format!("local_to_speech_embedding_mlps.{i}")),
                )
            })
            .collect::<candle_core::Result<_>>()?;
        let norms = (0..=config.n_vq)
            .map(|i| rms_norm(d, 1e-6, vb.pp(format!("layer_norm_before_lm_heads.{i}"))))
            .collect::<candle_core::Result<_>>()?;
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
        Ok(Self {
            config,
            tokenizer,
            embeddings,
            global,
            depth,
            to_depth,
            from_depth,
            norms,
            heads,
            device: device.clone(),
        })
    }
    fn embed(&self, rows: &[Vec<u32>]) -> anyhow::Result<Tensor> {
        let mut result = None;
        for (channel, embedding) in self.embeddings.iter().enumerate() {
            let ids: Vec<_> = rows.iter().map(|row| row[channel]).collect();
            let value =
                embedding.forward(&Tensor::from_vec(ids, (1, rows.len()), &self.device)?)?;
            result = Some(match result {
                None => value,
                Some(previous) => (previous + value)?,
            });
        }
        Ok(result.expect("validated codebooks"))
    }
    /// Emit full RVQ frames; `false` cancels generation without claiming EOS.
    pub fn generate(
        &mut self,
        request: &crate::Generation<'_>,
        cancelled: &impl Fn() -> bool,
        mut frame: impl FnMut(&[u32]) -> anyhow::Result<bool>,
    ) -> anyhow::Result<()> {
        let crate::Generation {
            text,
            instruction,
            reference,
            max_frames,
            seed,
        } = *request;
        self.global.reset();
        self.depth.reset();
        let rows = crate::prompt::text_prompt(
            &self.tokenizer,
            &self.config,
            text,
            instruction,
            reference,
        )?;
        let embedded = self.embed(&rows)?;
        let mut hidden = self.global.forward(&embedded, cancelled)?;
        let mut history: Vec<Vec<u32>> = (0..=self.config.n_vq)
            .map(|i| rows.iter().map(|row| row[i]).collect())
            .collect();
        let mut rng = StdRng::seed_from_u64(seed);
        for _ in 0..max_frames {
            crate::check_cancel(cancelled)?;
            self.depth.reset();
            let last = hidden.narrow(1, hidden.dim(1)? - 1, 1)?;
            let mut input = self.to_depth.forward(&last)?;
            let mut row = Vec::with_capacity(self.config.n_vq + 1);
            for (channel, channel_history) in history.iter().enumerate() {
                let local = self.depth.forward(&input, cancelled)?;
                let projected =
                    self.norms[channel].forward(&self.from_depth[channel].forward(&local)?)?;
                let logits = self.heads[channel].forward(&projected)?;
                let token = crate::sampling::sample(
                    &logits,
                    channel_history,
                    if channel == 0 {
                        crate::sampling::Parameters::TEXT
                    } else {
                        crate::sampling::Parameters::LOCAL
                    },
                    if channel == 0 {
                        None
                    } else {
                        Some(self.config.audio_pad_code)
                    },
                    &mut rng,
                )?;
                row.push(token);
                if channel == 0 && token == self.config.audio_end_token_id {
                    return Ok(());
                }
                input = self.to_depth.forward(
                    &self.embeddings[channel].forward(&Tensor::new(&[[token]], &self.device)?)?,
                )?;
            }
            anyhow::ensure!(
                row[0] == self.config.audio_assistant_gen_slot_token_id,
                "unexpected MOSS text/control token {}",
                row[0]
            );
            if !frame(&row[1..])? {
                anyhow::bail!("MOSS inference cancelled");
            }
            for (h, token) in history.iter_mut().zip(&row) {
                h.push(*token);
            }
            hidden = self.global.forward(&self.embed(&[row])?, cancelled)?;
        }
        anyhow::bail!("MOSS frame limit reached before EOS")
    }
}
