//! FP32 ONNX OmniVoice graphs with the existing Rust text frontend.
use crate::onnx_runtime::{Generator, OnnxBackend, Request, send};
use ::omnivoice::{
    contracts::{GenerationRequest, I64Tensor2, VoiceClonePrompt},
    frontend::Frontend,
    stage0_loop,
};
use ort::{session::Session, value::Tensor};
use rand::{Rng, SeedableRng, rngs::StdRng};
use std::path::{Path, PathBuf};
use tts_core::backend::{AudioChunk, BackendError, Pcm};
use tts_protocol::{Capabilities, Device, Event};
pub const EXPORT_REVISION: &str = "4ec8125833a6a2806ff2e07b31e73f106954aad0";
pub fn directory(root: &Path) -> PathBuf {
    root.join("omnivoice-onnx/models/0.6b")
        .join(crate::omnivoice::resources::REVISION)
}
pub fn voice_store(root: &Path) -> anyhow::Result<tts_core::voices::VoiceStore> {
    // Source WAV/text are shared; ONNX encoded prompts are cached separately.
    crate::omnivoice::voice_store(&crate::omnivoice::directory(root))
}
pub fn capabilities(root: &Path) -> anyhow::Result<Capabilities> {
    let mut caps = crate::omnivoice::capabilities(&crate::omnivoice::directory(root))?;
    caps.backend = "omnivoice-onnx".into();
    caps.model_name.push_str(" · ONNX（实验）");
    Ok(caps)
}
pub async fn prepare(
    root: &Path,
    progress: tokio::sync::mpsc::Sender<Event>,
) -> anyhow::Result<()> {
    crate::onnx_runtime::verify_export(
        &directory(root),
        EXPORT_REVISION,
        crate::omnivoice::resources::REVISION,
        &[
            "config.json",
            "tokenizer.json",
            "omnivoice_lm/model.onnx",
            "audio_tokenizer_encoder/model.onnx",
            "audio_tokenizer_decoder/model.onnx",
        ],
        progress,
    )
    .await
}
pub async fn load(root: &Path, device: Device) -> Result<OnnxBackend, BackendError> {
    let root = root.to_owned();
    let caps = capabilities(&root).map_err(|e| BackendError::Initialize(e.to_string()))?;
    OnnxBackend::load(caps, directory(&root), device, move |directory, device| {
        let session = |name| {
            crate::devices::session(
                &directory.join(name).join("model.onnx"),
                device,
                &directory.join("coreml").join(name),
            )
        };
        Ok(Box::new(Omni {
            lm: session("omnivoice_lm")?,
            // With pinned ORT/CoreML, scalar shape partitions corrupt Slice
            // ends in this encoder. Keep reference encoding on CPU explicitly;
            // LM/decoder still register the requested provider without retries.
            enc: crate::devices::session(
                &directory.join("audio_tokenizer_encoder/model.onnx"),
                if device == Device::Coreml {
                    Device::Cpu
                } else {
                    device
                },
                &directory.join("coreml/audio_tokenizer_encoder"),
            )?,
            dec: session("audio_tokenizer_decoder")?,
            frontend: Frontend::from_onnx_directory(&directory)?,
            root,
            directory,
        }))
    })
    .await
}
struct Omni {
    lm: Session,
    enc: Session,
    dec: Session,
    frontend: Frontend,
    root: PathBuf,
    directory: PathBuf,
}
#[derive(serde::Serialize, serde::Deserialize)]
struct CachedPrompt {
    source_sha256: String,
    transcript: String,
    tokens: Vec<i64>,
    frames: usize,
    rms: f32,
}
impl Omni {
    fn prompt(&mut self, voice: &str) -> anyhow::Result<VoiceClonePrompt> {
        use sha2::{Digest, Sha256};
        let store = voice_store(&self.root)?;
        let record = store.load(voice)?;
        let path = store.path(voice)?;
        let bytes = std::fs::read(path.join("reference.wav"))?;
        let hash = format!("{:x}", Sha256::digest(&bytes));
        let cache_dir = self.directory.join("prompts").join(EXPORT_REVISION);
        std::fs::create_dir_all(&cache_dir)?;
        let cache = cache_dir.join(format!("{hash}.json"));
        let cached = if cache.is_file() {
            let cached: CachedPrompt = serde_json::from_reader(std::fs::File::open(&cache)?)?;
            anyhow::ensure!(
                cached.source_sha256 == hash
                    && cached.transcript == record.transcript
                    && cached.frames > 0
                    && cached.frames <= 750
                    && cached.tokens.len() == 8 * cached.frames
                    && cached.tokens.iter().all(|id| (0..1024).contains(id))
                    && cached.rms.is_finite(),
                "invalid ONNX clone cache"
            );
            cached
        } else {
            let mut samples = crate::reference::load(&path.join("reference.wav"), 24000)?;
            let rms = (samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32).sqrt();
            anyhow::ensure!(rms.is_finite() && rms > 0.0, "empty reference audio");
            if rms < 0.1 {
                for sample in &mut samples {
                    *sample *= 0.1 / rms;
                }
            }
            let outputs = self.enc.run(
                ort::inputs!["audio"=>Tensor::from_array((vec![1,1,samples.len()],samples))?],
            )?;
            let (shape, tokens) = outputs["audio_codes"].try_extract_tensor::<i64>()?;
            anyhow::ensure!(
                shape.len() == 3 && shape[0] == 1 && shape[1] == 8,
                "invalid Omni encoder shape"
            );
            let cached = CachedPrompt {
                source_sha256: hash,
                transcript: record.transcript,
                tokens: tokens.to_vec(),
                frames: shape[2] as usize,
                rms,
            };
            anyhow::ensure!(
                cached.frames > 0
                    && cached.frames <= 750
                    && cached.tokens.iter().all(|id| (0..1024).contains(id)),
                "invalid Omni encoder tokens"
            );
            let temporary = tempfile::NamedTempFile::new_in(&cache_dir)?;
            serde_json::to_writer(temporary.as_file(), &cached)?;
            temporary.persist(cache)?;
            cached
        };
        Ok(VoiceClonePrompt {
            ref_audio_tokens: I64Tensor2::new((8, cached.frames), cached.tokens)?,
            ref_text: cached.transcript,
            ref_rms: Some(cached.rms),
        })
    }
}
impl Generator for Omni {
    fn generate(&mut self, request: &Request, cancelled: &dyn Fn() -> bool) -> anyhow::Result<()> {
        let mut input = GenerationRequest::new_text_only(&request.text).with_language("zh");
        let mut rms = None;
        if request.voice.starts_with("custom:") {
            let prompt = self.prompt(&request.voice)?;
            rms = prompt.ref_rms;
            input = input.with_voice_clone_prompt(prompt);
        }
        let task = self.frontend.build_task(&input)?;
        anyhow::ensure!(
            task.target_lens.len() == 1 && task.target_lens[0] <= 750,
            "Omni segment exceeds generation budget"
        );
        let target = task.target_lens[0];
        let prompt = self.frontend.prepare_prompt(&task, 0)?;
        let mut batch = stage0_loop::pack_cfg_batch(&[prompt], &[target])?;
        let (_, channels, seq) = batch.batch_input_ids.dims();
        let mut trace = std::env::var_os("NOVEL_TTS_ONNX_TRACE").map(|path| (PathBuf::from(path),
            serde_json::json!({"input_ids":batch.batch_input_ids.data,
                "audio_mask":batch.batch_audio_mask.data,"attention_mask":batch.batch_attention_mask.data,
                "seq":seq,"target":target,"channels":channels})));
        let config = &input.generation_config;
        let mut rng = StdRng::seed_from_u64(42);
        let mut tokens = vec![1024i64; channels * target];
        for step in 0..config.num_step {
            anyhow::ensure!(!cancelled(), "inference cancelled");
            let positions: Vec<i64> = (0..2).flat_map(|_| (0..seq).map(|v| v as i64)).collect();
            let outputs=self.lm.run(ort::inputs![
                "input_ids"=>Tensor::from_array((vec![2,channels,seq],batch.batch_input_ids.data.clone()))?,
                "audio_mask"=>Tensor::from_array((vec![2,seq],batch.batch_audio_mask.data.clone()))?,
                "attention_mask"=>Tensor::from_array((vec![2,1,seq,seq],batch.batch_attention_mask.data.clone()))?,
                "position_ids"=>Tensor::from_array((vec![2,seq],positions))?
            ])?;
            let (shape, logits) = outputs["logits"].try_extract_tensor::<f32>()?;
            anyhow::ensure!(
                shape.as_ref() == [2, channels as i64, seq as i64, 1025]
                    && logits.iter().all(|v| v.is_finite()),
                "invalid Omni logits"
            );
            if step == 0
                && let Some((_, data)) = &mut trace
            {
                data["first_logits"] = serde_json::json!(logits);
            }
            let mut cond = Vec::with_capacity(channels * target * 1025);
            let mut uncond = Vec::with_capacity(cond.capacity());
            for ch in 0..channels {
                let start = (ch * seq + seq - target) * 1025;
                cond.extend_from_slice(&logits[start..start + target * 1025]);
                let start = (channels * seq + ch * seq) * 1025;
                uncond.extend_from_slice(&logits[start..start + target * 1025]);
            }
            let prediction = stage0_loop::predict_tokens_with_scoring(
                &cond,
                &uncond,
                config.guidance_scale,
                0.0,
                1024,
                channels,
                1025,
            )?;
            let count = batch.schedules[0][step];
            if count == 0 {
                continue;
            }
            let mut scores = prediction.confidence_scores;
            for (index, score) in scores.iter_mut().enumerate() {
                *score -= ((index / target) as f32) * config.layer_penalty_factor;
                let u = rng.random::<f32>().clamp(1e-10, 1.0 - 1e-10);
                *score += config.position_temperature * (-(-u.ln() + 1e-10).ln());
            }
            stage0_loop::apply_step_updates(
                &mut tokens,
                &prediction.pred_tokens,
                &scores,
                1024,
                count,
            )?;
            for ch in 0..channels {
                for t in 0..target {
                    let token = tokens[ch * target + t];
                    batch.batch_input_ids.set(0, ch, seq - target + t, token);
                    batch.batch_input_ids.set(1, ch, t, token);
                }
            }
        }
        anyhow::ensure!(
            tokens.iter().all(|id| (0..1024).contains(id)),
            "Omni left invalid or masked audio tokens"
        );
        anyhow::ensure!(!cancelled(), "inference cancelled");
        if let Some((_, data)) = &mut trace {
            data["codes"] = serde_json::json!(tokens);
        }
        let outputs = self.dec.run(
            ort::inputs!["audio_codes"=>Tensor::from_array((vec![1,channels,target],tokens))?],
        )?;
        let raw = outputs["audio"].try_extract_tensor::<f32>()?.1.to_vec();
        if let Some((path, mut data)) = trace.take() {
            data["raw_audio"] = serde_json::json!(raw);
            serde_json::to_writer(std::fs::File::create(path)?, &data)?;
        }
        anyhow::ensure!(
            raw.len() == target * 960 && raw.iter().all(|v| v.is_finite()),
            "invalid Omni decoded audio"
        );
        let trimmed = ::omnivoice::postprocess::remove_silence(&raw, 24000, 500, 100, 100);
        let raw = if trimmed.is_empty() { raw } else { trimmed };
        let normalized = if let Some(rms) = rms {
            ::omnivoice::postprocess::apply_clone_rms_restore(&raw, rms)
        } else {
            ::omnivoice::postprocess::peak_normalize_auto_voice(&raw)?
        };
        let pcm = Pcm {
            samples: ::omnivoice::postprocess::fade_and_pad_audio(&normalized, 24000, 0.1, 0.1),
            sample_rate: 24000,
            channels: 1,
        };
        pcm.duration_ms()?;
        send(request, Ok(AudioChunk::Pcm(pcm)), cancelled)?;
        send(request, Ok(AudioChunk::End), cancelled)
    }
}
