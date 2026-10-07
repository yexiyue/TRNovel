//! Experimental FP32 CustomVoice inference using the pinned upstream ONNX ABI.
use crate::onnx_runtime::{Generator, OnnxBackend, Request, send};
use ort::{
    session::Session,
    value::{DynValue, Tensor},
};
use rand::{Rng, SeedableRng, rngs::StdRng};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tts_core::backend::{AudioChunk, BackendError, Pcm};
use tts_protocol::{Capabilities, Device, Event};

pub const EXPORT_REVISION: &str = "5d1687f02855ee403792ed145fabc936f62b60f8";
pub fn model(id: Option<&str>) -> anyhow::Result<crate::qwen::models::Model> {
    let model = crate::qwen::models::Model::parse(id)?;
    anyhow::ensure!(
        matches!(
            model,
            crate::qwen::models::Model::Custom06 | crate::qwen::models::Model::Custom17
        ),
        "ONNX experiment supports CustomVoice only"
    );
    Ok(model)
}
pub fn directory(root: &Path, model: crate::qwen::models::Model) -> PathBuf {
    root.join("qwen-onnx/models")
        .join(model.id())
        .join(model.revision())
}
pub fn capabilities(model: crate::qwen::models::Model) -> Capabilities {
    let mut caps = crate::qwen::model_capabilities(model);
    caps.backend = "qwen-onnx".into();
    caps.model_name.push_str(" · ONNX（实验）");
    caps
}
pub async fn prepare(
    root: &Path,
    model: crate::qwen::models::Model,
    progress: tokio::sync::mpsc::Sender<Event>,
) -> anyhow::Result<()> {
    crate::onnx_runtime::verify_export(
        &directory(root, model),
        EXPORT_REVISION,
        model.revision(),
        &[
            "tokenizer.json",
            "config.json",
            "onnx/text_project/text_project.onnx",
            "onnx/codec_embed/codec_embed.onnx",
            "onnx/talker/talker_core.onnx",
            "onnx/decode/sub_talker_sample.onnx",
            "onnx/tokenizer/tokenizer12hz_decode_chunk.onnx",
        ],
        progress,
    )
    .await
}
pub async fn load(
    root: &Path,
    model: crate::qwen::models::Model,
    device: Device,
) -> Result<OnnxBackend, BackendError> {
    OnnxBackend::load(
        capabilities(model),
        directory(root, model),
        device,
        |directory, device| Ok(Box::new(Qwen::load(&directory, device)?)),
    )
    .await
}
fn floats(shape: impl Into<Vec<usize>>, data: Vec<f32>) -> anyhow::Result<DynValue> {
    let shape = shape.into();
    if shape.contains(&0) {
        anyhow::ensure!(data.is_empty(), "nonempty zero-length tensor");
        return Ok(Tensor::<f32>::new(
            &ort::memory::Allocator::default(),
            shape.iter().map(|v| *v as i64).collect::<Vec<_>>(),
        )?
        .into_dyn());
    }
    Ok(Tensor::from_array((shape, data))?.into_dyn())
}
fn ints(shape: impl Into<Vec<usize>>, data: Vec<i64>) -> anyhow::Result<DynValue> {
    Ok(Tensor::from_array((shape.into(), data))?.into_dyn())
}
fn vector(value: &DynValue) -> anyhow::Result<Vec<f32>> {
    Ok(value.try_extract_tensor::<f32>()?.1.to_vec())
}
struct Qwen {
    text: Session,
    codec: Session,
    core: Session,
    residual: Session,
    decoder: Session,
    config: Value,
    tokenizer: tokenizers::Tokenizer,
    hidden: usize,
    matched_control: bool,
}
struct Prompt {
    embeds: Vec<f32>,
    pad: Vec<f32>,
    trailing: Vec<f32>,
}
impl Qwen {
    fn load(directory: &Path, device: Device) -> anyhow::Result<Self> {
        let matched_control = std::env::var_os("NOVEL_TTS_QWEN_GREEDY_CONTROL").is_some();
        let session = |relative: &str| {
            crate::devices::session(
                &directory.join("onnx").join(relative),
                device,
                &directory.join("coreml").join(relative.replace('/', "_")),
            )
        };
        let config: Value =
            serde_json::from_reader(std::fs::File::open(directory.join("config.json"))?)?;
        let hidden = config["talker_config"]["hidden_size"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("missing Qwen hidden size"))?
            as usize;
        Ok(Self {
            text: session("text_project/text_project.onnx")?,
            codec: session("codec_embed/codec_embed.onnx")?,
            core: session("talker/talker_core.onnx")?,
            residual: if matched_control {
                crate::devices::session(
                    &directory.join("onnx-greedy/decode/sub_talker_sample.onnx"),
                    device,
                    &directory.join("coreml/greedy_control"),
                )?
            } else {
                session("decode/sub_talker_sample.onnx")?
            },
            decoder: session("tokenizer/tokenizer12hz_decode_chunk.onnx")?,
            tokenizer: tokenizers::Tokenizer::from_file(directory.join("tokenizer.json"))
                .map_err(|e| anyhow::anyhow!(e.to_string()))?,
            config,
            hidden,
            matched_control,
        })
    }
    fn id(&self, key: &str) -> anyhow::Result<i64> {
        self.config["talker_config"][key]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("missing Qwen token {key}"))
    }
    fn text_embed(&mut self, ids: &[i64]) -> anyhow::Result<Vec<f32>> {
        let out = self
            .text
            .run(ort::inputs!["input_ids" => ints(vec![1, ids.len()], ids.to_vec())?])?;
        vector(&out["text_embed"])
    }
    fn codec_embed(&mut self, ids: &[i64]) -> anyhow::Result<Vec<f32>> {
        let out = self.codec.run(ort::inputs!["token_ids" => ints(vec![1, ids.len()], ids.to_vec())?, "ref_code" => ints(vec![1,1,16], vec![0;16])?])?;
        vector(&out["embed"])
    }
    fn tokenize(&self, text: &str) -> anyhow::Result<Vec<i64>> {
        Ok(self
            .tokenizer
            .encode(text, false)
            .map_err(|e| anyhow::anyhow!(e.to_string()))?
            .get_ids()
            .iter()
            .map(|v| i64::from(*v))
            .collect())
    }
    fn prompt(&mut self, request: &Request) -> anyhow::Result<Prompt> {
        let h = self.hidden;
        let ids = self.tokenize(&format!(
            "<|im_start|>assistant\n{}<|im_end|>\n<|im_start|>assistant\n",
            request.text
        ))?;
        anyhow::ensure!(ids.len() > 8, "invalid Qwen text prompt");
        let special = self.text_embed(&[
            self.config["tts_bos_token_id"]
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("missing tts_bos"))?,
            self.config["tts_eos_token_id"]
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("missing tts_eos"))?,
            self.config["tts_pad_token_id"]
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("missing tts_pad"))?,
        ])?;
        let pad = special[2 * h..3 * h].to_vec();
        let language = if request
            .text
            .chars()
            .any(|c| matches!(c, '\u{3400}'..='\u{9fff}'))
        {
            "chinese"
        } else {
            "english"
        };
        let language = self.config["talker_config"]["spk_is_dialect"][&request.voice]
            .as_str()
            .filter(|_| language == "chinese")
            .unwrap_or(language);
        let language_id = self.config["talker_config"]["codec_language_id"][language]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("unknown language"))?;
        let mut codec_ids = vec![
            self.id("codec_think_id")?,
            self.id("codec_think_bos_id")?,
            language_id,
            self.id("codec_think_eos_id")?,
        ];
        codec_ids.push(
            self.config["talker_config"]["spk_id"][&request.voice]
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("unknown Qwen speaker"))?,
        );
        codec_ids.extend([self.id("codec_pad_id")?, self.id("codec_bos_id")?]);
        let condition = self.codec_embed(&codec_ids)?;
        let mut prompt = Vec::new();
        if let Some(style) = &request.style {
            let instruct = self.tokenize(&format!("<|im_start|>user\n{style}<|im_end|>\n"))?;
            prompt.extend(self.text_embed(&instruct)?);
        }
        prompt.extend(self.text_embed(&ids[..3])?);
        for i in 0..codec_ids.len() - 1 {
            let text = if i == codec_ids.len() - 2 {
                &special[..h]
            } else {
                &pad
            };
            prompt.extend(
                condition[i * h..(i + 1) * h]
                    .iter()
                    .zip(text)
                    .map(|(a, b)| a + b),
            );
        }
        if self.matched_control {
            // Match Candle's first-token prefill and incremental text
            // conditioning as well as its greedy residual codebooks.
            let first = self.text_embed(&ids[3..4])?;
            prompt.extend(
                first
                    .iter()
                    .zip(&condition[(codec_ids.len() - 1) * h..])
                    .map(|(a, b)| a + b),
            );
            let mut trailing = self.text_embed(&ids[4..ids.len() - 5])?;
            trailing.extend_from_slice(&special[h..2 * h]);
            return Ok(Prompt {
                embeds: prompt,
                pad,
                trailing,
            });
        }
        let mut text = self.text_embed(&ids[3..ids.len() - 5])?;
        text.extend_from_slice(&special[h..2 * h]);
        let codec_pad = self.codec_embed(&vec![self.id("codec_pad_id")?; text.len() / h])?;
        prompt.extend(text.iter().zip(codec_pad).map(|(a, b)| a + b));
        prompt.extend(
            pad.iter()
                .zip(&condition[(codec_ids.len() - 1) * h..])
                .map(|(a, b)| a + b),
        );
        Ok(Prompt {
            embeds: prompt,
            trailing: pad.clone(),
            pad,
        })
    }
    fn core_step(
        &mut self,
        embeds: DynValue,
        past: Vec<DynValue>,
        position: usize,
        length: usize,
    ) -> anyhow::Result<(Vec<f32>, DynValue, Vec<DynValue>)> {
        let total = position + length;
        let mask: Vec<f32> = (0..length)
            .flat_map(|q| (0..total).map(move |k| if k <= position + q { 0.0 } else { f32::MIN }))
            .collect();
        let mut inputs = vec![
            ("inputs_embeds".into(), embeds),
            (
                "attention_mask".into(),
                floats(vec![1, 1, length, total], mask)?,
            ),
            (
                "cache_position".into(),
                ints(vec![length], (position..total).map(|v| v as i64).collect())?,
            ),
        ];
        for (index, value) in past.into_iter().enumerate() {
            inputs.push((
                format!(
                    "past_{}_{}",
                    if index % 2 == 0 { "key" } else { "value" },
                    index / 2
                ),
                value,
            ));
        }
        let mut outputs = self.core.run(inputs)?;
        let logits = vector(&outputs["logits"])?;
        let hidden = outputs
            .remove("last_hidden")
            .ok_or_else(|| anyhow::anyhow!("missing last_hidden"))?;
        let mut cache = Vec::new();
        for layer in 0..self.config["talker_config"]["num_hidden_layers"]
            .as_u64()
            .unwrap_or(0)
        {
            for kind in ["key", "value"] {
                cache.push(
                    outputs
                        .remove(format!("new_past_{kind}_{layer}"))
                        .ok_or_else(|| anyhow::anyhow!("missing cache"))?,
                );
            }
        }
        Ok((logits, hidden, cache))
    }
    fn decode(&mut self, codes: &[i64], emitted: usize) -> anyhow::Result<Pcm> {
        let frames = codes.len() / 16;
        let start = emitted.saturating_sub(25);
        let n = frames - start;
        let mask = (0..n)
            .flat_map(|q| (0..n).map(move |k| if k <= q && k + 72 > q { 0.0 } else { f32::MIN }))
            .collect();
        let mut cos = Vec::with_capacity(n * 64);
        let mut sin = Vec::with_capacity(n * 64);
        for pos in 0..n {
            for i in 0..64 {
                let angle = pos as f32 / 10000f32.powf((i % 32 * 2) as f32 / 64.0);
                cos.push(angle.cos());
                sin.push(angle.sin());
            }
        }
        let out = self.decoder.run(ort::inputs![
            "audio_codes" => ints(vec![1,n,16],codes[start*16..].to_vec())?, "context_frames" => ints(vec![],vec![(emitted-start) as i64])?,
            "attention_mask" => floats(vec![1,1,n,n],mask)?, "rope_cos" => floats(vec![1,n,64],cos)?, "rope_sin" => floats(vec![1,n,64],sin)?
        ])?;
        let pcm = Pcm {
            samples: vector(&out["audio_values"])?,
            sample_rate: 24000,
            channels: 1,
        };
        pcm.duration_ms()?;
        anyhow::ensure!(
            pcm.samples.len() == (frames - emitted) * 1920,
            "unexpected Qwen codec sample count"
        );
        Ok(pcm)
    }
}
impl Generator for Qwen {
    fn generate(&mut self, request: &Request, cancelled: &dyn Fn() -> bool) -> anyhow::Result<()> {
        let Prompt {
            embeds: prompt,
            pad,
            trailing,
        } = self.prompt(request)?;
        let mut trace = std::env::var_os("NOVEL_TTS_ONNX_TRACE").map(|path| {
            (
                PathBuf::from(path),
                serde_json::json!({"text":request.text,"voice":request.voice,"style":request.style,
                "prompt":prompt,"pad":pad,"trailing":trailing,"hidden_size":self.hidden,
                "non_streaming_mode":!self.matched_control}),
            )
        });
        let cfg = &self.config["talker_config"];
        let layers = cfg["num_hidden_layers"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("missing layers"))? as usize;
        let heads = cfg["num_key_value_heads"]
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("missing heads"))? as usize;
        let dim = cfg["head_dim"]
            .as_u64()
            .unwrap_or((self.hidden as u64) / cfg["num_attention_heads"].as_u64().unwrap_or(1))
            as usize;
        let past = (0..layers * 2)
            .map(|_| floats(vec![1, heads, 0, dim], Vec::new()))
            .collect::<anyhow::Result<Vec<_>>>()?;
        let position = prompt.len() / self.hidden;
        let (mut logits, mut hidden, mut cache) = self.core_step(
            floats(vec![1, position, self.hidden], prompt)?,
            past,
            0,
            position,
        )?;
        if let Some((_, data)) = &mut trace {
            data["first_logits"] = serde_json::json!(logits);
        }
        let mut rng = StdRng::seed_from_u64(42);
        let mut codes = Vec::new();
        let mut history = Vec::new();
        let mut emitted = 0;
        let eos = self.id("codec_eos_token_id")?;
        let vocab = self.config["talker_config"]["vocab_size"]
            .as_u64()
            .unwrap_or(0) as usize;
        anyhow::ensure!(
            vocab >= 2048 && logits.len() >= vocab && eos >= 2048 && (eos as usize) < vocab,
            "invalid Qwen logits"
        );
        for (position, step) in (position..).zip(0..375) {
            anyhow::ensure!(!cancelled(), "inference cancelled");
            let mut scores = logits[logits.len() - vocab..].to_vec();
            for (id, score) in scores.iter_mut().enumerate().skip(2048) {
                if id as i64 != eos {
                    *score = f32::NEG_INFINITY;
                }
            }
            if step < 2 {
                scores[eos as usize] = f32::NEG_INFINITY;
            }
            let token = sample(&scores, &history, rng.random::<f64>())?;
            if token == eos {
                anyhow::ensure!(!codes.is_empty(), "Qwen generated empty audio");
                if let Some((path, mut data)) = trace.take() {
                    data["codes"] = serde_json::json!(codes);
                    serde_json::to_writer(std::fs::File::create(path)?, &data)?;
                }
                if emitted < codes.len() / 16 {
                    send(
                        request,
                        Ok(AudioChunk::Pcm(self.decode(&codes, emitted)?)),
                        cancelled,
                    )?;
                }
                return send(request, Ok(AudioChunk::End), cancelled);
            }
            history.push(token);
            let text_embed = trailing
                .get(step * self.hidden..(step + 1) * self.hidden)
                .unwrap_or(&pad)
                .to_vec();
            let mut out = self.residual.run(ort::inputs!["first_token"=>ints(vec![1],vec![token])?, "last_hidden"=>hidden, "text_embed"=>floats(vec![1,1,self.hidden],text_embed)?, "residual_random_u"=>floats(vec![1,15],(0..15).map(|_|rng.random::<f32>()).collect())?])?;
            let frame = out["codebook_tokens"]
                .try_extract_tensor::<i64>()?
                .1
                .to_vec();
            anyhow::ensure!(
                frame.len() == 16 && frame.iter().all(|id| (0..2048).contains(id)),
                "invalid Qwen codec tokens"
            );
            codes.extend(frame);
            let embed = out
                .remove("decode_embed")
                .ok_or_else(|| anyhow::anyhow!("missing decode_embed"))?;
            drop(out);
            (logits, hidden, cache) = self.core_step(embed, cache, position, 1)?;
            if codes.len() / 16 - emitted >= 10 {
                send(
                    request,
                    Ok(AudioChunk::Pcm(self.decode(&codes, emitted)?)),
                    cancelled,
                )?;
                emitted = codes.len() / 16;
            }
        }
        anyhow::bail!("Qwen generation frame budget exhausted before EOS")
    }
}
fn sample(logits: &[f32], history: &[i64], uniform: f64) -> anyhow::Result<i64> {
    anyhow::ensure!(
        (0.0..1.0).contains(&uniform),
        "invalid sampling random value"
    );
    anyhow::ensure!(
        logits.iter().all(|v| !v.is_nan() && *v != f32::INFINITY),
        "invalid Qwen logits"
    );
    let mut scores = logits.to_vec();
    for id in history
        .iter()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
    {
        let score = &mut scores[id as usize];
        *score = if *score < 0.0 {
            *score * 1.05
        } else {
            *score / 1.05
        };
    }
    let mut candidates: Vec<_> = scores
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, v)| v.is_finite())
        .collect();
    candidates.sort_by(|a, b| b.1.total_cmp(&a.1));
    candidates.truncate(50);
    anyhow::ensure!(!candidates.is_empty(), "empty sampling distribution");
    let max = candidates[0].1;
    let sum: f64 = candidates
        .iter()
        .map(|(_, v)| f64::from((*v - max) / 0.9).exp())
        .sum();
    let mut cumulative = 0.0;
    let keep = candidates
        .iter()
        .position(|(_, v)| {
            cumulative += f64::from((*v - max) / 0.9).exp() / sum;
            cumulative > 0.9
        })
        .map_or(candidates.len(), |i| i + 1);
    candidates.truncate(keep);
    candidates.sort_by_key(|(id, _)| *id);
    let sum: f64 = candidates
        .iter()
        .map(|(_, v)| f64::from((*v - max) / 0.9).exp())
        .sum();
    let mut remaining = uniform * sum;
    for (id, v) in &candidates {
        remaining -= f64::from((*v - max) / 0.9).exp();
        if remaining < 0.0 {
            return Ok(*id as i64);
        }
    }
    Ok(candidates[candidates.len() - 1].0 as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sampler_rejects_invalid_logits_and_preserves_suppression() {
        assert!(sample(&[f32::NAN, 0.0], &[], 0.5).is_err());
        assert!(sample(&[f32::NEG_INFINITY], &[], 0.5).is_err());
        for uniform in [0.0, 0.5, 0.99999] {
            assert_eq!(sample(&[f32::NEG_INFINITY, 4.0], &[], uniform).unwrap(), 1);
        }
    }
    #[test]
    fn experimental_models_reject_clone_and_design() {
        assert!(model(Some("1.7b-base")).is_err());
        assert!(model(Some("1.7b-voicedesign")).is_err());
        assert_eq!(
            capabilities(model(Some("1.7b-customvoice")).unwrap()).default_voice,
            "uncle_fu"
        );
    }
    #[test]
    fn prefill_cache_accepts_zero_length_sequence() {
        let value = floats(vec![1, 8, 0, 128], Vec::new()).unwrap();
        // ORT returns a null data pointer for zero elements; rc.10 must not
        // extract a Rust slice from this tensor. Shape metadata is sufficient.
        assert_eq!(
            value.dtype().tensor_shape().unwrap().as_ref(),
            [1, 8, 0, 128]
        );
        assert!(floats(vec![1, 8, 0, 128], vec![1.0]).is_err());
    }
}
