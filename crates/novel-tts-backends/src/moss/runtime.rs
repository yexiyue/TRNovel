//! CPU ONNX generation. Native sessions and caches remain on the inference thread.
use super::{
    diagnostics::{GenerationEnd, GenerationStats},
    voices::VoiceStore,
};
use ort::{
    session::{Session, SessionInputValue},
    value::{DynValue, Tensor},
};
use rand::Rng;
use serde_json::Value;
use std::{collections::HashMap, path::Path};
use tts_core::backend::{AudioChunk, Pcm};

type Feeds = HashMap<String, DynValue>;
fn ints(shape: impl Into<Vec<usize>>, data: Vec<i32>) -> anyhow::Result<DynValue> {
    Ok(Tensor::from_array((shape.into(), data))?.into_dyn())
}
fn floats(shape: impl Into<Vec<usize>>, data: Vec<f32>) -> anyhow::Result<DynValue> {
    Ok(Tensor::from_array((shape.into(), data))?.into_dyn())
}
fn run(session: &mut Session, feeds: Feeds, device: tts_protocol::Device) -> anyhow::Result<Feeds> {
    let mut binding = session.create_binding()?;
    let mut outputs = if device == tts_protocol::Device::Cuda {
        use ort::memory::{AllocationDevice, AllocatorType, MemoryInfo, MemoryType};
        let gpu = MemoryInfo::new(
            AllocationDevice::CUDA,
            0,
            AllocatorType::Device,
            MemoryType::Default,
        )?;
        let cpu = MemoryInfo::default();
        for (name, value) in &feeds {
            binding.bind_input(name, value)?;
        }
        for output in session.outputs() {
            let cached = output.name().starts_with("present_") || output.name().contains("_out_");
            binding.bind_output_to_device(output.name(), if cached { &gpu } else { &cpu })?;
        }
        session.run_binding(&binding)?
    } else {
        session.run(
            feeds
                .into_iter()
                .map(|(name, value)| (name, SessionInputValue::from(value)))
                .collect::<Vec<_>>(),
        )?
    };
    let names = outputs.keys().map(str::to_owned).collect::<Vec<_>>();
    Ok(names
        .into_iter()
        .map(|name| {
            let value = outputs.remove(&name).expect("enumerated output");
            (name, value)
        })
        .collect())
}
fn take(outputs: &mut Feeds, name: &str) -> anyhow::Result<DynValue> {
    outputs
        .remove(name)
        .ok_or_else(|| anyhow::anyhow!("missing ONNX output {name}"))
}
fn length(outputs: &Feeds, name: &str) -> anyhow::Result<usize> {
    let (_, values) = outputs
        .get(name)
        .ok_or_else(|| anyhow::anyhow!("missing {name}"))?
        .try_extract_tensor::<i32>()?;
    usize::try_from(
        *values
            .first()
            .ok_or_else(|| anyhow::anyhow!("empty {name}"))?,
    )
    .map_err(Into::into)
}
fn hidden(outputs: &mut Feeds) -> anyhow::Result<DynValue> {
    let value = take(outputs, "global_hidden")?;
    let (shape, data) = value.try_extract_tensor::<f32>()?;
    let width = usize::try_from(
        *shape
            .last()
            .ok_or_else(|| anyhow::anyhow!("empty hidden shape"))?,
    )?;
    anyhow::ensure!(data.len() >= width && width > 0, "invalid hidden state");
    floats(vec![1, width], data[data.len() - width..].to_vec())
}
fn global_cache(outputs: Feeds) -> Feeds {
    outputs
        .into_iter()
        .filter(|(name, _)| name.starts_with("present_"))
        .map(|(name, value)| (name.replacen("present_", "past_", 1), value))
        .collect()
}
fn session(
    path: &Path,
    cancelled: &impl Fn() -> bool,
    device: tts_protocol::Device,
    cache: &Path,
) -> anyhow::Result<Session> {
    anyhow::ensure!(!cancelled(), "model loading cancelled");
    crate::devices::session(path, device, cache)
}

pub(super) struct Runtime {
    prefill: Session,
    decode: Session,
    local: Session,
    codec: Session,
    encoder: Session,
    manifest: Value,
    codec_meta: Value,
    directory: std::path::PathBuf,
    device: tts_protocol::Device,
}
impl Runtime {
    pub(super) fn load_on(
        directory: &Path,
        cancelled: impl Fn() -> bool,
        device: tts_protocol::Device,
    ) -> anyhow::Result<Self> {
        let cache = directory.join("coreml-cache");
        let tts = directory.join("tts");
        let codec = directory.join("codec");
        Ok(Self {
            device,
            prefill: session(
                &tts.join("moss_tts_prefill.onnx"),
                &cancelled,
                device,
                &cache,
            )?,
            decode: session(
                &tts.join("moss_tts_decode_step.onnx"),
                &cancelled,
                device,
                &cache,
            )?,
            local: session(
                &tts.join("moss_tts_local_fixed_sampled_frame.onnx"),
                &cancelled,
                device,
                &cache,
            )?,
            codec: session(
                &codec.join("moss_audio_tokenizer_decode_step.onnx"),
                &cancelled,
                device,
                &cache,
            )?,
            encoder: session(
                &codec.join("moss_audio_tokenizer_encode.onnx"),
                &cancelled,
                device,
                &cache,
            )?,
            manifest: serde_json::from_str(include_str!("assets/browser_poc_manifest.json"))?,
            codec_meta: serde_json::from_str(include_str!("assets/codec_browser_onnx_meta.json"))?,
            directory: directory.to_owned(),
        })
    }
    fn rows(&self, tokens: &[i32], codes: &[Vec<i32>]) -> anyhow::Result<Vec<i32>> {
        let config = &self.manifest["tts_config"];
        let templates = &self.manifest["prompt_templates"];
        let mut rows = Vec::new();
        let text_row = |rows: &mut Vec<i32>, token: i32| {
            rows.push(token);
            rows.extend([1024; 16]);
        };
        let append = |rows: &mut Vec<i32>, key: &str| -> anyhow::Result<()> {
            for token in templates[key]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("invalid prompt template"))?
            {
                text_row(
                    rows,
                    token
                        .as_i64()
                        .ok_or_else(|| anyhow::anyhow!("invalid token"))?
                        as i32,
                );
            }
            Ok(())
        };
        append(&mut rows, "user_prompt_prefix_token_ids")?;
        text_row(
            &mut rows,
            config["audio_start_token_id"].as_i64().unwrap_or(6) as i32,
        );
        for code in codes {
            anyhow::ensure!(code.len() == 16, "invalid voice code width");
            rows.push(8);
            rows.extend(code);
        }
        text_row(&mut rows, 7);
        append(&mut rows, "user_prompt_after_reference_token_ids")?;
        for &token in tokens {
            text_row(&mut rows, token);
        }
        append(&mut rows, "assistant_prompt_prefix_token_ids")?;
        text_row(&mut rows, 6);
        Ok(rows)
    }
    fn codec_state(&self) -> anyhow::Result<(Feeds, Vec<(String, String)>)> {
        let mut state = Feeds::new();
        let mut mapping = Vec::new();
        let shape = |spec: &Value, key: &str| -> anyhow::Result<Vec<usize>> {
            spec[key]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("missing shape"))?
                .iter()
                .map(|n| {
                    n.as_u64()
                        .map(|n| n as usize)
                        .ok_or_else(|| anyhow::anyhow!("invalid shape"))
                })
                .collect()
        };
        for spec in self.codec_meta["streaming_decode"]["transformer_offsets"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("missing codec state"))?
        {
            let dims = shape(spec, "shape")?;
            let input = spec["input_name"].as_str().unwrap().to_owned();
            let output = spec["output_name"].as_str().unwrap().to_owned();
            state.insert(
                input.clone(),
                ints(dims.clone(), vec![0; dims.iter().product()])?,
            );
            mapping.push((input, output));
        }
        for spec in self.codec_meta["streaming_decode"]["attention_caches"]
            .as_array()
            .unwrap()
        {
            for (prefix, shape_key, is_float, fill) in [
                ("offset", "offset_shape", false, 0),
                ("cached_keys", "cache_shape", true, 0),
                ("cached_values", "cache_shape", true, 0),
                ("cached_positions", "positions_shape", false, -1),
            ] {
                let input = spec[format!("{prefix}_input_name")]
                    .as_str()
                    .unwrap()
                    .to_owned();
                let output = spec[format!("{prefix}_output_name")]
                    .as_str()
                    .unwrap()
                    .to_owned();
                let dims = shape(spec, shape_key)?;
                let value = if is_float {
                    floats(dims.clone(), vec![0.; dims.iter().product()])?
                } else {
                    ints(dims.clone(), vec![fill; dims.iter().product()])?
                };
                state.insert(input.clone(), value);
                mapping.push((input, output));
            }
        }
        Ok((state, mapping))
    }
    fn audio(
        &mut self,
        frames: &[i32],
        state: &mut Feeds,
        mapping: &[(String, String)],
    ) -> anyhow::Result<Pcm> {
        let count = frames.len() / 16;
        let mut feeds = std::mem::take(state);
        feeds.insert(
            "audio_codes".into(),
            ints(vec![1, count, 16], frames.to_vec())?,
        );
        feeds.insert(
            "audio_code_lengths".into(),
            ints(vec![1], vec![count as i32])?,
        );
        let mut outputs = run(&mut self.codec, feeds, self.device)?;
        let len = length(&outputs, "audio_lengths")?;
        let value = take(&mut outputs, "audio")?;
        let (shape, data) = value.try_extract_tensor::<f32>()?;
        anyhow::ensure!(
            shape.len() == 3 && shape[0] == 1 && shape[1] == 2,
            "invalid codec PCM shape"
        );
        let stride = usize::try_from(shape[2])?;
        anyhow::ensure!(len > 0 && len <= stride, "invalid codec audio length");
        let samples = (0..len).flat_map(|i| [data[i], data[stride + i]]).collect();
        for (input, output) in mapping {
            state.insert(input.clone(), take(&mut outputs, output)?);
        }
        Ok(Pcm {
            samples,
            sample_rate: 48000,
            channels: 2,
        })
    }
    pub(super) fn import_voice(
        &mut self,
        id: String,
        name: String,
        wav: &Path,
    ) -> anyhow::Result<()> {
        let waveform = super::audio::read_wav(wav)?;
        let len = waveform.len() / 2;
        let mut outputs = run(
            &mut self.encoder,
            HashMap::from([
                ("waveform".into(), floats(vec![1, 2, len], waveform)?),
                ("input_lengths".into(), ints(vec![1], vec![len as i32])?),
            ]),
            self.device,
        )?;
        let length = length(&outputs, "audio_code_lengths")?;
        let value = take(&mut outputs, "audio_codes")?;
        let (_, data) = value.try_extract_tensor::<i32>()?;
        anyhow::ensure!(
            length > 0 && length * 16 <= data.len(),
            "invalid encoded reference audio"
        );
        let codes = data[..length * 16]
            .as_chunks::<16>()
            .0
            .iter()
            .map(|row| row.to_vec())
            .collect();
        VoiceStore::new(&self.directory).save(id, name, codes)
    }
    pub(super) fn generate(
        &mut self,
        tokens: Vec<i32>,
        voice: &str,
        seed: Option<u64>,
        tx: &tokio::sync::mpsc::Sender<Result<AudioChunk, tts_core::backend::BackendError>>,
        stats: &mut GenerationStats,
    ) -> anyhow::Result<GenerationEnd> {
        let codes = VoiceStore::new(&self.directory).codes(voice, &self.manifest)?;
        let rows = self.rows(&tokens, &codes)?;
        let count = rows.len() / 17;
        let mut outputs = run(
            &mut self.prefill,
            HashMap::from([
                ("input_ids".into(), ints(vec![1, count, 17], rows)?),
                (
                    "attention_mask".into(),
                    ints(vec![1, count], vec![1; count])?,
                ),
            ]),
            self.device,
        )?;
        let mut global_hidden = hidden(&mut outputs)?;
        let mut cache = global_cache(outputs);
        let mut seen = vec![0i32; 16 * 1024];
        let mut pending = Vec::new();
        let mut fixed = seed.map(|seed| seed as u32);
        let mut rng = rand::rng();
        let mut uniform = || {
            if let Some(state) = &mut fixed {
                *state = state.wrapping_mul(1664525).wrapping_add(1013904223);
                (*state >> 8) as f32 / 16777216.0
            } else {
                rng.random::<f32>().min(0.99999994)
            }
        };
        let (mut codec_state, mapping) = self.codec_state()?;
        for (valid_length, step) in (count..).zip(0..375) {
            if tx.is_closed() {
                return Ok(GenerationEnd::Cancelled);
            }
            let mut sampled = run(
                &mut self.local,
                HashMap::from([
                    ("global_hidden".into(), global_hidden),
                    (
                        "repetition_seen_mask".into(),
                        ints(vec![1, 16, 1024], seen.clone())?,
                    ),
                    (
                        "assistant_random_u".into(),
                        floats(vec![1], vec![uniform()])?,
                    ),
                    (
                        "audio_random_u".into(),
                        floats(vec![1, 16], (0..16).map(|_| uniform()).collect())?,
                    ),
                ]),
                self.device,
            )?;
            if length(&sampled, "should_continue")? == 0 {
                break;
            }
            let frame = take(&mut sampled, "frame_token_ids")?;
            let (_, data) = frame.try_extract_tensor::<i32>()?;
            anyhow::ensure!(data.len() == 16, "invalid generated frame");
            for (i, &token) in data.iter().enumerate() {
                anyhow::ensure!((0..1024).contains(&token), "invalid audio token");
                seen[i * 1024 + token as usize] = 1;
            }
            stats.frames += 1;
            pending.extend_from_slice(data);
            if pending.len() >= 16 * 3 {
                let pcm = self.audio(&pending, &mut codec_state, &mapping)?;
                stats.audio_seconds += pcm.duration_ms()? as f64 / 1000.0;
                if tx.blocking_send(Ok(AudioChunk::Pcm(pcm))).is_err() {
                    return Ok(GenerationEnd::Cancelled);
                }
                pending.clear();
            }
            let mut row = vec![9];
            row.extend_from_slice(data);
            cache.insert("input_ids".into(), ints(vec![1, 1, 17], row)?);
            cache.insert(
                "past_valid_lengths".into(),
                ints(vec![1], vec![valid_length as i32])?,
            );
            let mut decoded = run(&mut self.decode, cache, self.device)?;
            global_hidden = hidden(&mut decoded)?;
            cache = global_cache(decoded);
            if step == 374 {
                return Ok(GenerationEnd::FrameLimit);
            }
        }
        if !pending.is_empty() && !tx.is_closed() {
            let pcm = self.audio(&pending, &mut codec_state, &mapping)?;
            stats.audio_seconds += pcm.duration_ms()? as f64 / 1000.0;
            if tx.blocking_send(Ok(AudioChunk::Pcm(pcm))).is_err() {
                return Ok(GenerationEnd::Cancelled);
            }
        }
        if tx.blocking_send(Ok(AudioChunk::End)).is_err() {
            return Ok(GenerationEnd::Cancelled);
        }
        Ok(GenerationEnd::Eos)
    }
}
