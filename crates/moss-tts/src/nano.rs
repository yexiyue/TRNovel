//! Experimental native MOSS-TTS-Nano (GPT2 global/local, 16 RVQ channels).
//! Architecture follows OpenMOSS/MOSS-TTS-Nano, with attention conventions
//! cross-checked against ramishi/moss-tts-nano-rust-candle (Apache-2.0).
mod sampling;
mod transformer;
use candle_core::{DType, Device, IndexOp, Module, Tensor};
use candle_nn::{Embedding, Linear, VarBuilder, embedding};
use serde::Deserialize;
use std::path::Path;
use transformer::{Config as TransformerConfig, Transformer};

#[derive(Deserialize)]
struct Config {
    gpt2_config: TransformerConfig,
    n_vq: usize,
    audio_codebook_sizes: Vec<usize>,
    local_transformer_layers: usize,
}

pub struct Nano {
    global: Transformer,
    local: Transformer,
    text: Embedding,
    audio: Vec<Embedding>,
    text_head: Linear,
    audio_heads: Vec<Linear>,
    device: Device,
}
impl Nano {
    pub fn load(directory: &Path, device: &Device) -> anyhow::Result<Self> {
        let config: Config =
            serde_json::from_slice(&std::fs::read(directory.join("config.json"))?)?;
        anyhow::ensure!(
            config.n_vq == 16 && config.audio_codebook_sizes == vec![1024; 16],
            "unsupported Nano RVQ configuration"
        );
        // Candle reads the official BF16 PyTorch checkpoint and converts to F32.
        let vb = VarBuilder::from_pth(directory.join("pytorch_model.bin"), DType::F32, device)?;
        let c = &config.gpt2_config;
        let text = embedding(c.vocab_size, c.n_embd, vb.pp("transformer.wte"))?;
        let text_head = Linear::new(text.embeddings().clone(), None);
        let audio = (0..16)
            .map(|i| embedding(1024, c.n_embd, vb.pp(format!("audio_embeddings.{i}"))))
            .collect::<candle_core::Result<Vec<_>>>()?;
        let audio_heads = (0..16)
            .map(|i| {
                Ok(Linear::new(
                    vb.get((1024, c.n_embd), &format!("audio_lm_heads.{i}.weight"))?,
                    None,
                ))
            })
            .collect::<candle_core::Result<Vec<_>>>()?;
        let global = Transformer::load(c, vb.pp("transformer"))?;
        let mut local_config = c.clone();
        local_config.n_layer = config.local_transformer_layers;
        let local = Transformer::load(&local_config, vb.pp("local_transformer"))?;
        Ok(Self {
            global,
            local,
            text,
            audio,
            text_head,
            audio_heads,
            device: device.clone(),
        })
    }
    fn embeds(&self, rows: &[Vec<u32>]) -> anyhow::Result<Tensor> {
        anyhow::ensure!(
            !rows.is_empty()
                && rows
                    .iter()
                    .all(|r| r.len() == 17 && r[0] < 16384 && r[1..].iter().all(|id| *id <= 1024)),
            "invalid Nano prompt rows"
        );
        let ids = Tensor::from_vec(
            rows.iter().map(|r| r[0]).collect::<Vec<_>>(),
            (1, rows.len()),
            &self.device,
        )?;
        let mut x = self.text.forward(&ids)?;
        for (channel, emb) in self.audio.iter().enumerate() {
            let ids = Tensor::from_vec(
                rows.iter().map(|r| r[channel + 1]).collect::<Vec<_>>(),
                (1, rows.len()),
                &self.device,
            )?;
            let mask = ids.ne(1024u32)?;
            let safe = mask.where_cond(&ids, &Tensor::zeros_like(&ids)?)?;
            x = (&x
                + emb
                    .forward(&safe)?
                    .broadcast_mul(&mask.to_dtype(DType::F32)?.unsqueeze(2)?)?)?;
        }
        Ok(x)
    }
    /// Emits complete 16-channel frames. Success requires explicit model EOS.
    pub fn generate(
        &mut self,
        rows: &[Vec<u32>],
        max_frames: usize,
        seed: u64,
        cancelled: &impl Fn() -> bool,
        mut frame: impl FnMut(Vec<u32>) -> anyhow::Result<()>,
    ) -> anyhow::Result<()> {
        self.global.reset();
        let mut x = self.embeds(rows)?;
        let mut history = vec![Vec::new(); 16];
        let mut rng = sampling::Sampler::new(seed);
        for _ in 0..max_frames {
            crate::check_cancel(cancelled)?;
            let hidden = self.global.forward(&x, cancelled)?;
            self.local.reset();
            let hidden = hidden.i((.., hidden.dim(1)? - 1, ..))?.unsqueeze(1)?;
            let local = self.local.forward(&hidden, cancelled)?.squeeze(1)?;
            let logits = self.text_head.forward(&local)?.flatten_all()?;
            let candidates = Tensor::cat(&[logits.narrow(0, 9, 1)?, logits.narrow(0, 7, 1)?], 0)?;
            let token = rng.sample(&candidates, &[], 1., 2, 1., 1.)?;
            if token == 1 {
                return Ok(());
            }
            let mut x_local = self.text.forward(&Tensor::new(&[[9u32]], &self.device)?)?;
            let mut codes = Vec::with_capacity(16);
            for (i, previous) in history.iter_mut().enumerate() {
                crate::check_cancel(cancelled)?;
                let hidden = self.local.forward(&x_local, cancelled)?.squeeze(1)?;
                let logits = self.audio_heads[i].forward(&hidden)?;
                let code = rng.sample(&logits, previous, 0.8, 25, 0.95, 1.2)?;
                previous.push(code);
                codes.push(code);
                x_local = self.audio[i].forward(&Tensor::new(&[[code]], &self.device)?)?;
            }
            let mut row = vec![9];
            row.extend_from_slice(&codes);
            frame(codes)?;
            x = self.embeds(&[row])?;
        }
        anyhow::bail!("Nano frame limit reached before end-of-speech")
    }
}
