use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use candle_core::{DType, Device, IndexOp, Module, Tensor, quantized::QMatMul};
use candle_nn::{Embedding, Linear, VarBuilder};
use rand::{Rng, SeedableRng, rngs::StdRng};
use safetensors::SafeTensors;
use serde::Deserialize;

use crate::codec_layers::QuantizedEmbedding;
use crate::{
    artifacts::{GeneratorArtifacts, GgufBundleArtifacts, RuntimeArtifacts},
    contracts::{
        BatchedInputs, F32Tensor3, F32Tensor4, I64Tensor2, I64Tensor3, PreparedInferenceBatch,
    },
    error::{OmniVoiceError, Result},
    runtime::RuntimeOptions,
    stage0_loop::{build_timesteps, build_unmask_schedules},
    stage0_qwen3::{Stage0ForwardPass, Stage0Qwen3Backbone, Stage0Qwen3Config},
    stage0_qwen3_quantized::{Stage0Qwen3QuantizedBackbone, keep_quantized_matmul},
};

#[derive(Debug, Clone, Deserialize)]
pub struct LocalQwen3Config {
    pub hidden_size: usize,
    pub intermediate_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub num_key_value_heads: usize,
    pub head_dim: usize,
    pub max_position_embeddings: usize,
    pub rms_norm_eps: f64,
    #[serde(default)]
    pub attention_bias: bool,
    #[serde(default = "default_hidden_act")]
    pub hidden_act: String,
    pub rope_parameters: RopeParameters,
    pub vocab_size: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RopeParameters {
    pub rope_theta: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Stage0Config {
    pub audio_vocab_size: usize,
    pub audio_mask_id: i64,
    pub num_audio_codebook: usize,
    pub llm_config: LocalQwen3Config,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage0DeterministicConfig {
    pub num_step: usize,
    pub guidance_scale: f32,
    pub t_shift: f32,
    pub layer_penalty_factor: f32,
    pub position_temperature: f32,
    pub class_temperature: f32,
    pub capture_steps: Vec<usize>,
    pub capture_layers: Vec<usize>,
    pub capture_final_hidden: bool,
}

const MAX_GENERATION_STEPS: usize = 4096;

impl Default for Stage0DeterministicConfig {
    fn default() -> Self {
        Self {
            num_step: 32,
            guidance_scale: 2.0,
            t_shift: 0.1,
            layer_penalty_factor: 5.0,
            position_temperature: 0.0,
            class_temperature: 0.0,
            capture_steps: vec![0, 15, 31],
            capture_layers: vec![0, 13, 27],
            capture_final_hidden: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage0StepDebugCapture {
    pub step: usize,
    pub c_logits: F32Tensor4,
    pub u_logits: F32Tensor4,
    pub pred_tokens: I64Tensor3,
    pub confidence_scores: F32Tensor3,
    pub batch_input_ids_before_update: I64Tensor3,
    pub tokens_after_step: I64Tensor3,
    pub batch_input_ids_before_step: I64Tensor3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage0DebugCapture {
    pub inputs_embeds: F32Tensor3,
    pub hidden_layers: BTreeMap<usize, F32Tensor3>,
    pub final_hidden: F32Tensor3,
    pub steps: Vec<Stage0StepDebugCapture>,
    pub final_tokens: I64Tensor2,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage0ParityMetric {
    pub exact_match: bool,
    pub max_abs: f32,
    pub mae: f32,
    pub rmse: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage0ParityMetrics {
    pub metrics: BTreeMap<String, Stage0ParityMetric>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage0DebugRun {
    pub tokens: I64Tensor2,
    pub debug_capture: Stage0DebugCapture,
    pub parity_metrics: Stage0ParityMetrics,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage0GenerationOutput {
    pub tokens: Vec<I64Tensor2>,
    pub debug_capture: Option<Stage0DebugCapture>,
}

#[derive(Debug)]
struct Stage0DeviceGenerationOutput {
    tokens: Vec<Tensor>,
    debug_capture: Option<Stage0DebugCapture>,
}

fn default_hidden_act() -> String {
    "silu".to_string()
}

impl LocalQwen3Config {
    pub fn hidden_act(&self) -> Result<candle_nn::Activation> {
        match self.hidden_act.as_str() {
            "silu" => Ok(candle_nn::Activation::Silu),
            other => Err(OmniVoiceError::Unsupported(format!(
                "unsupported Qwen3 hidden activation {other}"
            ))),
        }
    }

    fn to_stage0_qwen3_config(&self) -> Result<Stage0Qwen3Config> {
        Ok(Stage0Qwen3Config {
            vocab_size: self.vocab_size,
            hidden_size: self.hidden_size,
            intermediate_size: self.intermediate_size,
            num_hidden_layers: self.num_hidden_layers,
            num_attention_heads: self.num_attention_heads,
            head_dim: self.head_dim,
            attention_bias: self.attention_bias,
            num_key_value_heads: self.num_key_value_heads,
            max_position_embeddings: self.max_position_embeddings,
            rope_theta: self.rope_parameters.rope_theta,
            rms_norm_eps: self.rms_norm_eps,
            hidden_act: self.hidden_act()?,
        })
    }
}

impl Stage0Config {
    pub fn from_model_root(model_root: impl AsRef<Path>) -> Result<Self> {
        let runtime = RuntimeArtifacts::from_model_root(model_root)?;
        Self::from_runtime_artifacts(&runtime)
    }

    pub fn from_artifacts(generator: &GeneratorArtifacts) -> Result<Self> {
        Ok(serde_json::from_str(&fs::read_to_string(
            generator.config_path(),
        )?)?)
    }

    pub fn from_runtime_artifacts(runtime: &RuntimeArtifacts) -> Result<Self> {
        if let Some(gguf) = runtime.gguf_bundle() {
            return Self::from_gguf_bundle(gguf);
        }
        Self::from_artifacts(runtime.generator()?)
    }

    pub fn from_gguf_bundle(gguf: &GgufBundleArtifacts) -> Result<Self> {
        let content = gguf.open_generator_content()?;
        let vocab_size = content
            .metadata
            .get("tokenizer.ggml.tokens")
            .and_then(|value| value.to_vec().ok())
            .map(|tokens| tokens.len())
            .ok_or_else(|| {
                OmniVoiceError::InvalidData(
                    "GGUF generator is missing tokenizer.ggml.tokens metadata".to_string(),
                )
            })?;
        Ok(Self {
            audio_vocab_size: gguf_metadata_usize(&content, "omnivoice.audio_vocab_size")?,
            audio_mask_id: gguf_metadata_usize(&content, "omnivoice.audio_mask_id")? as i64,
            num_audio_codebook: gguf_metadata_usize(&content, "omnivoice.num_audio_codebook")?,
            llm_config: LocalQwen3Config {
                hidden_size: gguf_metadata_usize(&content, "omnivoice-lm.embedding_length")?,
                intermediate_size: gguf_metadata_usize(
                    &content,
                    "omnivoice-lm.feed_forward_length",
                )?,
                num_hidden_layers: gguf_metadata_usize(&content, "omnivoice-lm.block_count")?,
                num_attention_heads: gguf_metadata_usize(
                    &content,
                    "omnivoice-lm.attention.head_count",
                )?,
                num_key_value_heads: gguf_metadata_usize(
                    &content,
                    "omnivoice-lm.attention.head_count_kv",
                )?,
                head_dim: gguf_metadata_usize(&content, "omnivoice-lm.attention.key_length")?,
                max_position_embeddings: 4096,
                rms_norm_eps: gguf_metadata_f64(
                    &content,
                    "omnivoice-lm.attention.layer_norm_rms_epsilon",
                )?,
                attention_bias: false,
                hidden_act: default_hidden_act(),
                rope_parameters: RopeParameters {
                    rope_theta: gguf_metadata_f64(&content, "omnivoice-lm.rope.freq_base")?,
                },
                vocab_size,
            },
        })
    }
}

#[derive(Debug, Clone)]
pub struct Stage0WeightLayout {
    weights_path: PathBuf,
    accepted_prefixes: BTreeSet<String>,
    ignored_keys: BTreeSet<String>,
}

impl Stage0WeightLayout {
    pub fn from_model_root(model_root: impl AsRef<Path>) -> Result<Self> {
        let runtime = RuntimeArtifacts::from_model_root(model_root)?;
        Self::from_runtime_artifacts(&runtime)
    }

    pub fn from_artifacts(generator: &GeneratorArtifacts) -> Result<Self> {
        Ok(Self {
            weights_path: generator.weights_path().to_path_buf(),
            accepted_prefixes: generator.observed_prefixes().clone(),
            ignored_keys: generator.ignored_keys().clone(),
        })
    }

    pub fn from_safetensors_file(
        path: impl AsRef<Path>,
        required_prefixes: &BTreeSet<String>,
        ignored_keys: &BTreeSet<String>,
    ) -> Result<Self> {
        let path = path.as_ref().to_path_buf();
        let mut accepted_prefixes = BTreeSet::new();
        let mut found_ignored_keys = BTreeSet::new();
        let bytes = fs::read(&path)?;
        let tensors = SafeTensors::deserialize(&bytes)?;
        for key in tensors.names() {
            if ignored_keys.contains(key) {
                found_ignored_keys.insert(key.to_string());
                continue;
            }
            if let Some(prefix) = key.split('.').next() {
                accepted_prefixes.insert(prefix.to_string());
            }
        }
        if accepted_prefixes != *required_prefixes {
            return Err(OmniVoiceError::InvalidData(format!(
                "stage0 weight prefixes {:?} do not match required {:?}",
                accepted_prefixes, required_prefixes
            )));
        }
        Ok(Self {
            weights_path: path,
            accepted_prefixes,
            ignored_keys: found_ignored_keys,
        })
    }

    pub fn from_runtime_artifacts(runtime: &RuntimeArtifacts) -> Result<Self> {
        if let Some(gguf) = runtime.gguf_bundle() {
            return Ok(Self {
                weights_path: gguf.generator_path().to_path_buf(),
                accepted_prefixes: BTreeSet::new(),
                ignored_keys: BTreeSet::new(),
            });
        }
        Self::from_artifacts(runtime.generator()?)
    }

    pub fn accepted_prefixes(&self) -> &BTreeSet<String> {
        &self.accepted_prefixes
    }

    pub fn ignored_keys(&self) -> &BTreeSet<String> {
        &self.ignored_keys
    }

    pub fn weights_path(&self) -> &Path {
        &self.weights_path
    }
}

#[derive(Debug)]
enum Stage0AudioEmbeddings {
    Dense(Embedding),
    Quantized(QuantizedEmbedding),
}

impl Stage0AudioEmbeddings {
    fn forward(&self, input_ids: &Tensor) -> Result<Tensor> {
        match self {
            Self::Dense(layer) => layer.forward(input_ids).map_err(Into::into),
            Self::Quantized(layer) => layer.forward(input_ids),
        }
    }
}

#[derive(Debug)]
pub struct Stage0Model {
    backbone: Stage0BackboneKind,
    audio_embeddings: Stage0AudioEmbeddings,
    audio_heads: Stage0HeadProjection,
    codebook_layer_offsets: Tensor,
    num_audio_codebook: usize,
    audio_vocab_size: usize,
    activation_dtype: DType,
}

#[derive(Debug)]
enum Stage0BackboneKind {
    Dense(Stage0Qwen3Backbone),
    Quantized(Stage0Qwen3QuantizedBackbone),
}

impl Stage0BackboneKind {
    fn embed_text_tokens(&self, input_ids: &Tensor) -> Result<Tensor> {
        match self {
            Self::Dense(model) => model.embed_text_tokens(input_ids),
            Self::Quantized(model) => model.embed_text_tokens(input_ids),
        }
    }

    fn forward_embeds_with_flash(
        &self,
        input_embeds: &Tensor,
        attention_mask: Option<&Tensor>,
        capture_layers: &BTreeSet<usize>,
        flash_seqlens: Option<&[usize]>,
    ) -> Result<Stage0ForwardPass> {
        match self {
            Self::Dense(model) => model.forward_embeds_with_flash(
                input_embeds,
                attention_mask,
                capture_layers,
                flash_seqlens,
            ),
            Self::Quantized(model) => model.forward_embeds_with_flash(
                input_embeds,
                attention_mask,
                capture_layers,
                flash_seqlens,
            ),
        }
    }
}

#[derive(Debug)]
enum Stage0HeadProjection {
    Dense(Linear),
    Quantized(QMatMul),
}

impl Stage0HeadProjection {
    fn forward(&self, hidden: &Tensor) -> Result<Tensor> {
        match self {
            Self::Dense(layer) => layer.forward(hidden).map_err(Into::into),
            Self::Quantized(layer) => layer.forward(hidden).map_err(Into::into),
        }
    }
}

/// Thread-safe synchronous cancellation, independent of an async executor.
#[derive(Clone)]
pub struct CancellationProbe(pub Arc<dyn Fn() -> bool + Send + Sync>);
impl std::fmt::Debug for CancellationProbe {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CancellationProbe").finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct Stage0RuntimePlan {
    options: RuntimeOptions,
    config: Stage0Config,
    weight_layout: Stage0WeightLayout,
    device: Device,
    runtime_dtype: DType,
    model: OnceLock<std::result::Result<Stage0Model, String>>,
    cpu_seed: Mutex<Option<u64>>,
    cancellation: Mutex<Option<CancellationProbe>>,
}

impl Stage0RuntimePlan {
    pub fn from_options(options: RuntimeOptions) -> Result<Self> {
        let runtime = options.load_runtime_artifacts()?;
        Self::from_runtime_artifacts(options, &runtime)
    }

    pub fn from_runtime_artifacts(
        options: RuntimeOptions,
        runtime: &RuntimeArtifacts,
    ) -> Result<Self> {
        let device = options.resolve_device()?;
        Self::from_runtime_artifacts_with_device(options, runtime, device)
    }

    pub fn from_runtime_artifacts_with_device(
        options: RuntimeOptions,
        runtime: &RuntimeArtifacts,
        device: Device,
    ) -> Result<Self> {
        let runtime_dtype = options.resolve_stage0_dtype(&device, runtime)?;
        Ok(Self {
            config: Stage0Config::from_runtime_artifacts(runtime)?,
            weight_layout: Stage0WeightLayout::from_runtime_artifacts(runtime)?,
            device,
            runtime_dtype,
            cpu_seed: Mutex::new(options.seed()),
            cancellation: Mutex::new(None),
            options,
            model: OnceLock::new(),
        })
    }

    pub fn prepare_batch(
        &self,
        batched: &BatchedInputs,
        cond_lens: &[usize],
        target_lens: &[usize],
    ) -> Result<PreparedInferenceBatch> {
        if cond_lens.len() != target_lens.len() {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "cond_lens length {} does not match target_lens length {}",
                cond_lens.len(),
                target_lens.len()
            )));
        }

        validate_batched_inputs(
            batched,
            cond_lens,
            target_lens,
            self.config.num_audio_codebook,
            self.config.audio_mask_id,
        )?;

        let prepared = PreparedInferenceBatch {
            input_ids: batched.batch_input_ids.to_candle(&self.device)?,
            audio_mask: batched.batch_audio_mask.to_candle(&self.device)?,
            attention_mask: batched.batch_attention_mask.to_candle(&self.device)?,
            tokens_init: batched.tokens_init.to_candle(&self.device)?,
            target_lens: target_lens.to_vec(),
            cond_lens: cond_lens.to_vec(),
            runtime_dtype: self.runtime_dtype,
        };
        validate_prepared_batch(
            &prepared,
            self.config.num_audio_codebook,
            self.config.audio_mask_id,
        )?;
        Ok(prepared)
    }

    pub fn generate_deterministic(
        &self,
        prepared: &PreparedInferenceBatch,
        config: &Stage0DeterministicConfig,
        capture_steps: &[usize],
    ) -> Result<Stage0GenerationOutput> {
        self.run_loop(prepared, config, capture_steps)
    }

    pub fn generate_deterministic_device(
        &self,
        prepared: &PreparedInferenceBatch,
        config: &Stage0DeterministicConfig,
    ) -> Result<Vec<Tensor>> {
        Ok(self.run_loop_device(prepared, config, &[])?.tokens)
    }

    pub fn set_seed(&self, seed: u64) -> Result<()> {
        *self
            .cpu_seed
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = Some(seed);
        if self.device.is_cpu() {
            return Ok(());
        }
        self.device.set_seed(seed)?;
        Ok(())
    }

    pub fn set_cancellation_flag(&self, flag: Option<Arc<AtomicBool>>) {
        self.set_cancellation_probe(
            flag.map(|flag| CancellationProbe(Arc::new(move || flag.load(Ordering::Acquire)))),
        );
    }

    pub fn set_cancellation_probe(&self, probe: Option<CancellationProbe>) {
        *self
            .cancellation
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = probe;
    }

    pub fn cancellation_requested(&self) -> bool {
        self.cancellation
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .as_ref()
            .is_some_and(|probe| (probe.0)())
    }

    pub fn debug_case(
        &self,
        prepared: &PreparedInferenceBatch,
        config: &Stage0DeterministicConfig,
    ) -> Result<Stage0DebugCapture> {
        self.generate_deterministic(prepared, config, &config.capture_steps)?
            .debug_capture
            .ok_or_else(|| {
                OmniVoiceError::InvalidData(
                    "stage0 deterministic debug run did not capture debug tensors".to_string(),
                )
            })
    }

    pub fn config(&self) -> &Stage0Config {
        &self.config
    }

    pub fn weight_layout(&self) -> &Stage0WeightLayout {
        &self.weight_layout
    }

    pub fn options(&self) -> &RuntimeOptions {
        &self.options
    }

    pub fn device(&self) -> &Device {
        &self.device
    }

    pub fn runtime_dtype(&self) -> DType {
        self.runtime_dtype
    }

    pub fn is_loaded(&self) -> bool {
        self.model.get().is_some()
    }

    fn model(&self) -> Result<&Stage0Model> {
        let result = self.model.get_or_init(|| {
            Stage0Model::load(
                &self.config,
                self.weight_layout.weights_path(),
                &self.device,
                self.runtime_dtype,
            )
            .map_err(|error| error.to_string())
        });
        match result {
            Ok(model) => Ok(model),
            Err(message) => Err(OmniVoiceError::InvalidData(message.clone())),
        }
    }

    fn run_loop(
        &self,
        prepared: &PreparedInferenceBatch,
        config: &Stage0DeterministicConfig,
        capture_steps: &[usize],
    ) -> Result<Stage0GenerationOutput> {
        let output = self.run_loop_device(prepared, config, capture_steps)?;
        Ok(Stage0GenerationOutput {
            tokens: output
                .tokens
                .iter()
                .map(tensor_to_i64_tensor2)
                .collect::<Result<Vec<_>>>()?,
            debug_capture: output.debug_capture,
        })
    }

    fn run_loop_device(
        &self,
        prepared: &PreparedInferenceBatch,
        config: &Stage0DeterministicConfig,
        capture_steps: &[usize],
    ) -> Result<Stage0DeviceGenerationOutput> {
        validate_deterministic_config(config)?;
        validate_prepared_batch(
            prepared,
            self.config.num_audio_codebook,
            self.config.audio_mask_id,
        )?;
        if self.cancellation_requested() {
            return Err(OmniVoiceError::InvalidRequest(
                "inference cancelled".to_string(),
            ));
        }
        let seed = *self
            .cpu_seed
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if let Some(seed) = seed
            && !self.device.is_cpu()
        {
            // Still seed the device for any remaining on-device rand uses
            // (e.g. class_temperature > 0 token sampling).
            self.device.set_seed(seed)?;
        }
        // Position unmask sampling uses a host StdRng on ALL devices.
        //
        // This is NOT "run generation on CPU": the Qwen backbone + logits stay
        // on CUDA. Only the tiny Gumbel draw over confidence scores
        // (≤ codebooks * target_len floats per step) is sampled on the host.
        //
        // Candle CUDA `Tensor::rand` / `rand_like` Gumbel can drive the unmask
        // schedule into collapsed token sequences for some seeds/shapes
        // (listening 05: near-DC rumble; cross-decode proves stage1 is fine —
        // cuda tokens decode badly on both CPU and CUDA decoders).
        let mut host_rng = Some(if let Some(seed) = seed {
            StdRng::seed_from_u64(seed)
        } else {
            let mut rng = rand::rng();
            StdRng::from_rng(&mut rng)
        });
        let batch_size = prepared.target_lens.len();
        let debug_enabled = !capture_steps.is_empty();
        if batch_size != 1 && debug_enabled {
            return Err(OmniVoiceError::Unsupported(
                "stage0 debug capture currently supports one prompt per run".to_string(),
            ));
        }

        let model = self.model()?;
        let num_codebooks = self.config.num_audio_codebook;
        let capture_layers: BTreeSet<usize> = config.capture_layers.iter().copied().collect();
        let capture_steps: BTreeSet<usize> = capture_steps.iter().copied().collect();
        let attention_mask =
            model.prepare_attention_mask(&prepared.attention_mask, self.runtime_dtype)?;
        let timesteps = build_timesteps(0.0, 1.0, config.num_step, config.t_shift)?;
        let schedules = build_unmask_schedules(
            &prepared.target_lens,
            num_codebooks,
            &timesteps,
            config.num_step,
        )?;
        let layer_penalties = Tensor::from_vec(
            (0..num_codebooks)
                .map(|layer| layer as f32 * config.layer_penalty_factor)
                .collect::<Vec<_>>(),
            (1, num_codebooks, 1),
            &self.device,
        )?;
        let mut batch_input_ids = prepared.input_ids.clone();
        let mut tokens = prepared.tokens_init.clone();
        // Text positions never change during denoise; cache their embeddings once.
        let cached_text_embeds = model.cache_text_embeds(&batch_input_ids)?;
        let empty_capture_layers = BTreeSet::new();
        // Per-row active lengths for flash-attn: cond rows then uncond rows.
        let flash_seqlens: Vec<usize> = prepared
            .cond_lens
            .iter()
            .copied()
            .chain(prepared.target_lens.iter().copied())
            .collect();
        let max_target_len = prepared.target_lens.iter().copied().max().unwrap_or(0);
        // One host Gumbel draw + one H2D for the whole denoise loop (not per step).
        let mut gumbel_step_idx = 0usize;
        let gumbel_bank = if config.position_temperature > 0.0 {
            build_position_gumbel_bank(
                &schedules,
                num_codebooks,
                max_target_len,
                config.num_step,
                &self.device,
                host_rng.as_mut().expect("host rng"),
            )?
        } else {
            None
        };

        let initial_forward = if debug_enabled {
            Some(model.forward_with_text_cache(
                &batch_input_ids,
                &prepared.audio_mask,
                &attention_mask,
                &capture_layers,
                Some(&cached_text_embeds),
                Some(flash_seqlens.as_slice()),
            )?)
        } else {
            None
        };

        let mut step_captures = Vec::new();
        for step in 0..config.num_step {
            if self.cancellation_requested() {
                return Err(OmniVoiceError::InvalidRequest(
                    "inference cancelled".to_string(),
                ));
            }
            let forward = model.forward_with_text_cache(
                &batch_input_ids,
                &prepared.audio_mask,
                &attention_mask,
                &empty_capture_layers,
                Some(&cached_text_embeds),
                Some(flash_seqlens.as_slice()),
            )?;
            // Keep full logits in runtime dtype; only cast the (small) target
            // slices to f32 for scoring — avoids a full (2B,C,S,V) materialization.
            let batch_logits = &forward.logits;
            let step_needs_noise = schedules
                .iter()
                .take(batch_size)
                .any(|s| s.get(step).copied().unwrap_or(0) > 0);
            let shared_step_noise = if step_needs_noise {
                if let Some(bank) = gumbel_bank.as_ref() {
                    let noise = bank.i(gumbel_step_idx..gumbel_step_idx + 1)?;
                    gumbel_step_idx += 1;
                    Some(noise)
                } else {
                    None
                }
            } else {
                None
            };
            for (batch_index, batch_schedule) in schedules.iter().enumerate().take(batch_size) {
                let update_count = batch_schedule[step];
                if update_count == 0 {
                    // The Python reference skips prediction entirely when this
                    // step has no tokens to unmask.  Besides saving work, this
                    // preserves the RNG stream for later stochastic steps.
                    continue;
                }
                let target_len = prepared.target_lens[batch_index];
                let cond_len = prepared.cond_lens[batch_index];
                let c_logits = batch_logits
                    .i((
                        batch_index..batch_index + 1,
                        ..,
                        (cond_len - target_len)..cond_len,
                        ..,
                    ))?
                    .to_dtype(DType::F32)?;
                let u_logits = batch_logits
                    .i((
                        batch_size + batch_index..batch_size + batch_index + 1,
                        ..,
                        0..target_len,
                        ..,
                    ))?
                    .to_dtype(DType::F32)?;
                let batch_input_ids_before_update =
                    if debug_enabled && capture_steps.contains(&step) {
                        Some(batch_input_ids.clone())
                    } else {
                        None
                    };
                let (pred_tokens_tensor, confidence_scores_tensor) =
                    predict_tokens_with_scoring_from_tensors(
                        &c_logits,
                        &u_logits,
                        config.guidance_scale,
                        config.class_temperature,
                        self.config.audio_mask_id as usize,
                        host_rng.as_mut(),
                    )?;
                let current_tokens_view =
                    tokens.i((batch_index..batch_index + 1, .., 0..target_len))?;
                let step_noise = if let Some(noise) = shared_step_noise.as_ref() {
                    // bank row: (1,1,C,Tmax) → (1,C,T) to match confidence scores
                    let noise = noise.i((0, .., .., 0..target_len))?; // (1,C,T)
                    Some(noise)
                } else {
                    None
                };
                let updated_tokens = apply_step_updates_device(
                    &current_tokens_view,
                    &pred_tokens_tensor,
                    &confidence_scores_tensor,
                    self.config.audio_mask_id,
                    update_count,
                    &layer_penalties,
                    config.position_temperature,
                    host_rng.as_mut(),
                    step_noise.as_ref(),
                )?;
                tokens = tokens.slice_assign(
                    &[
                        batch_index..batch_index + 1,
                        0..num_codebooks,
                        0..target_len,
                    ],
                    &updated_tokens,
                )?;
                batch_input_ids = batch_input_ids.slice_assign(
                    &[
                        batch_index..batch_index + 1,
                        0..num_codebooks,
                        (cond_len - target_len)..cond_len,
                    ],
                    &updated_tokens,
                )?;
                batch_input_ids = batch_input_ids.slice_assign(
                    &[
                        batch_size + batch_index..batch_size + batch_index + 1,
                        0..num_codebooks,
                        0..target_len,
                    ],
                    &updated_tokens,
                )?;

                if debug_enabled && capture_steps.contains(&step) {
                    step_captures.push(Stage0StepDebugCapture {
                        step,
                        c_logits: tensor_to_f32_tensor4(&c_logits)?,
                        u_logits: tensor_to_f32_tensor4(&u_logits)?,
                        pred_tokens: tensor_to_i64_tensor3(&pred_tokens_tensor)?,
                        confidence_scores: tensor_to_f32_tensor3(&confidence_scores_tensor)?,
                        batch_input_ids_before_update: tensor_to_i64_tensor3(
                            batch_input_ids_before_update
                                .as_ref()
                                .expect("capture gate checked"),
                        )?,
                        tokens_after_step: tensor_to_i64_tensor3(&updated_tokens)?,
                        batch_input_ids_before_step: tensor_to_i64_tensor3(&batch_input_ids)?,
                    });
                }
            }
        }

        let mut final_tokens = Vec::with_capacity(batch_size);
        for (batch_index, target_len) in prepared.target_lens.iter().copied().enumerate() {
            final_tokens.push(tokens.i((batch_index, .., 0..target_len))?.contiguous()?);
        }

        let debug_capture = if !debug_enabled {
            None
        } else {
            let initial_forward = initial_forward.expect("debug capture checked");
            let mut hidden_layers = BTreeMap::new();
            for (index, tensor) in initial_forward.backbone.captured_hidden_layers.into_iter() {
                hidden_layers.insert(index, tensor_to_f32_tensor3(&tensor)?);
            }
            Some(Stage0DebugCapture {
                inputs_embeds: tensor_to_f32_tensor3(&initial_forward.inputs_embeds)?,
                hidden_layers,
                final_hidden: tensor_to_f32_tensor3(&initial_forward.backbone.final_hidden)?,
                steps: step_captures,
                final_tokens: tensor_to_i64_tensor2(
                    final_tokens
                        .first()
                        .expect("debug capture requires one prompt"),
                )?,
            })
        };

        Ok(Stage0DeviceGenerationOutput {
            tokens: final_tokens,
            debug_capture,
        })
    }
}

#[derive(Debug)]
pub struct Stage0ForwardOutputs {
    pub inputs_embeds: Tensor,
    pub backbone: Stage0ForwardPass,
    pub logits: Tensor,
}

impl Stage0Model {
    fn load(
        config: &Stage0Config,
        weights_path: &Path,
        device: &Device,
        dtype: DType,
    ) -> Result<Self> {
        if weights_path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
        {
            return Self::load_from_gguf(config, weights_path, device, dtype);
        }
        let vb = mmap_var_builder(weights_path, dtype, device)?;
        let llm_vb = vb.clone().rename_f(|name| {
            if let Some(stripped) = name.strip_prefix("model.") {
                format!("llm.{stripped}")
            } else {
                format!("llm.{name}")
            }
        });
        let backbone = Stage0BackboneKind::Dense(Stage0Qwen3Backbone::load(
            &config.llm_config.to_stage0_qwen3_config()?,
            llm_vb,
        )?);
        let audio_embeddings_weight = vb.get(
            (
                config.num_audio_codebook * config.audio_vocab_size,
                config.llm_config.hidden_size,
            ),
            "audio_embeddings.weight",
        )?;
        let audio_embeddings = Stage0AudioEmbeddings::Dense(Embedding::new(
            audio_embeddings_weight,
            config.llm_config.hidden_size,
        ));
        let audio_heads_weight = vb.get(
            (
                config.num_audio_codebook * config.audio_vocab_size,
                config.llm_config.hidden_size,
            ),
            "audio_heads.weight",
        )?;
        let audio_heads = Stage0HeadProjection::Dense(Linear::new(audio_heads_weight, None));
        let codebook_layer_offsets = Tensor::from_vec(
            AudioEmbeddingMixer::new(config.num_audio_codebook, config.audio_vocab_size)
                .layer_offsets(),
            (1, config.num_audio_codebook, 1),
            device,
        )?;
        Ok(Self {
            backbone,
            audio_embeddings,
            audio_heads,
            codebook_layer_offsets,
            num_audio_codebook: config.num_audio_codebook,
            audio_vocab_size: config.audio_vocab_size,
            activation_dtype: dtype,
        })
    }

    fn load_from_gguf(
        config: &Stage0Config,
        weights_path: &Path,
        device: &Device,
        dtype: DType,
    ) -> Result<Self> {
        let qvb = candle_transformers::quantized_var_builder::VarBuilder::from_gguf(
            weights_path,
            device,
        )?;
        let backbone = Stage0BackboneKind::Quantized(Stage0Qwen3QuantizedBackbone::load(
            &config.llm_config.to_stage0_qwen3_config()?,
            qvb.clone(),
            dtype,
        )?);
        // Candle 0.11+: gather rows from packed Q4 without materializing the full table.
        let audio_embeddings = Stage0AudioEmbeddings::Quantized(QuantizedEmbedding::new(
            qvb.get(
                (
                    config.num_audio_codebook * config.audio_vocab_size,
                    config.llm_config.hidden_size,
                ),
                "audio_embeddings.weight",
            )?,
            dtype,
            device.clone(),
        )?);
        // Route through the float-safe helper: on a float GGUF (F32/BF16 base
        // files) `audio_heads.weight` is not packed-quant, and a raw
        // `QMatMul::QTensor(...)` would hit candle's cuda dequantize_f16 bail.
        let audio_heads = Stage0HeadProjection::Quantized(keep_quantized_matmul(qvb.get(
            (
                config.num_audio_codebook * config.audio_vocab_size,
                config.llm_config.hidden_size,
            ),
            "audio_heads.weight",
        )?)?);
        let codebook_layer_offsets = Tensor::from_vec(
            AudioEmbeddingMixer::new(config.num_audio_codebook, config.audio_vocab_size)
                .layer_offsets(),
            (1, config.num_audio_codebook, 1),
            device,
        )?;
        Ok(Self {
            backbone,
            audio_embeddings,
            audio_heads,
            codebook_layer_offsets,
            num_audio_codebook: config.num_audio_codebook,
            audio_vocab_size: config.audio_vocab_size,
            activation_dtype: dtype,
        })
    }

    fn prepare_attention_mask(&self, mask: &Tensor, dtype: DType) -> Result<Tensor> {
        let dims = mask.dims();
        if dims.len() != 4 {
            return Err(OmniVoiceError::InvalidTensorShape {
                name: "stage0_attention_mask".to_string(),
                expected: "(B, 1, Q, K)".to_string(),
                actual: format!("{dims:?}"),
            });
        }
        let on_true = Tensor::zeros(mask.shape(), dtype, mask.device())?;
        let on_false =
            Tensor::new(f32::NEG_INFINITY, mask.device())?.broadcast_as(mask.shape().dims())?;
        mask.where_cond(&on_true, &on_false.to_dtype(dtype)?)
            .map_err(Into::into)
    }

    fn cache_text_embeds(&self, input_ids: &Tensor) -> Result<Tensor> {
        let text_ids = input_ids.i((.., 0, ..))?;
        self.backbone.embed_text_tokens(&text_ids)
    }

    fn prepare_embed_inputs(
        &self,
        input_ids: &Tensor,
        audio_mask: &Tensor,
        cached_text_embeds: Option<&Tensor>,
    ) -> Result<Tensor> {
        // Text token ids only change outside the audio region; the denoise loop
        // only mutates target audio tokens, so text embeddings can be reused.
        let text_embeds = match cached_text_embeds {
            Some(embeds) => embeds.clone(),
            None => self.cache_text_embeds(input_ids)?,
        };
        let shifted_ids = (input_ids
            .broadcast_mul(&audio_mask.unsqueeze(1)?.to_dtype(DType::I64)?)?
            + self
                .codebook_layer_offsets
                .broadcast_as(input_ids.shape().dims())?)?;
        let audio_embeds = self.audio_embeddings.forward(&shifted_ids)?.sum(1)?;
        let selection_mask = audio_mask
            .unsqueeze(candle_core::D::Minus1)?
            .broadcast_as(audio_embeds.shape().dims())?;
        selection_mask
            .where_cond(&audio_embeds, &text_embeds)?
            .to_dtype(self.activation_dtype)
            .map_err(Into::into)
    }

    fn forward_with_text_cache(
        &self,
        input_ids: &Tensor,
        audio_mask: &Tensor,
        attention_mask: &Tensor,
        capture_layers: &BTreeSet<usize>,
        cached_text_embeds: Option<&Tensor>,
        flash_seqlens: Option<&[usize]>,
    ) -> Result<Stage0ForwardOutputs> {
        let inputs_embeds = self.prepare_embed_inputs(input_ids, audio_mask, cached_text_embeds)?;
        let backbone = self.backbone.forward_embeds_with_flash(
            &inputs_embeds,
            Some(attention_mask),
            capture_layers,
            flash_seqlens,
        )?;
        let final_hidden = backbone.final_hidden.clone();
        let (batch_size, seq_len, _) = final_hidden.dims3()?;
        let logits = self
            .audio_heads
            .forward(&final_hidden)?
            .reshape((
                batch_size,
                seq_len,
                self.num_audio_codebook,
                self.audio_vocab_size,
            ))?
            .permute((0, 2, 1, 3))?;
        Ok(Stage0ForwardOutputs {
            inputs_embeds,
            backbone,
            logits,
        })
    }
}

#[derive(Debug, Clone)]
pub struct AudioEmbeddingMixer {
    num_audio_codebook: usize,
    audio_vocab_size: usize,
}

impl AudioEmbeddingMixer {
    pub fn new(num_audio_codebook: usize, audio_vocab_size: usize) -> Self {
        Self {
            num_audio_codebook,
            audio_vocab_size,
        }
    }

    pub fn layer_offsets(&self) -> Vec<i64> {
        (0..self.num_audio_codebook)
            .map(|index| (index * self.audio_vocab_size) as i64)
            .collect()
    }

    pub fn shifted_audio_id(&self, layer: usize, token_id: i64) -> Result<i64> {
        let Some(offset) = self.layer_offsets().get(layer).copied() else {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "layer {layer} out of range for {} codebooks",
                self.num_audio_codebook
            )));
        };
        let vocab_size = i64::try_from(self.audio_vocab_size).map_err(|_| {
            OmniVoiceError::InvalidRequest("audio vocabulary size does not fit i64".to_string())
        })?;
        if token_id < 0 || token_id >= vocab_size {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "audio token {token_id} is outside vocabulary size {}",
                self.audio_vocab_size
            )));
        }
        token_id.checked_add(offset).ok_or_else(|| {
            OmniVoiceError::InvalidRequest("shifted audio token id overflowed i64".to_string())
        })
    }
}

fn predict_tokens_with_scoring_from_tensors(
    c_logits: &Tensor,
    u_logits: &Tensor,
    guidance_scale: f32,
    class_temperature: f32,
    audio_mask_id: usize,
    cpu_rng: Option<&mut StdRng>,
) -> Result<(Tensor, Tensor)> {
    let log_probs = if guidance_scale != 0.0 {
        let c_log_probs = candle_nn::ops::log_softmax(c_logits, candle_core::D::Minus1)?;
        let u_log_probs = candle_nn::ops::log_softmax(u_logits, candle_core::D::Minus1)?;
        let guided = (&c_log_probs + ((&c_log_probs - &u_log_probs)? * guidance_scale as f64)?)?;
        candle_nn::ops::log_softmax(&guided, candle_core::D::Minus1)?
    } else {
        candle_nn::ops::log_softmax(c_logits, candle_core::D::Minus1)?
    };
    let log_probs = mask_audio_token(&log_probs, audio_mask_id)?;
    if log_probs.device().is_cpu() && class_temperature > 0.0 {
        let filtered = filter_top_k(&log_probs, 0.1)?;
        let pred_tokens = gumbel_argmax_cpu(&filtered, class_temperature, cpu_rng)?;
        let confidence_scores = log_probs.max(candle_core::D::Minus1)?;
        return Ok((pred_tokens, confidence_scores.to_dtype(DType::F32)?));
    }
    let pred_tokens = if class_temperature > 0.0 {
        let filtered = filter_top_k(&log_probs, 0.1)?;
        gumbel_argmax(&filtered, class_temperature)?
    } else {
        log_probs.argmax(candle_core::D::Minus1)?
    }
    .to_dtype(DType::I64)?;
    let confidence_scores = log_probs.max(candle_core::D::Minus1)?;
    Ok((pred_tokens, confidence_scores))
}

fn mask_audio_token(log_probs: &Tensor, audio_mask_id: usize) -> Result<Tensor> {
    let vocab_size = log_probs.dim(candle_core::D::Minus1)?;
    if audio_mask_id >= vocab_size {
        return Err(OmniVoiceError::InvalidRequest(format!(
            "audio mask token {audio_mask_id} is outside vocab size {vocab_size}"
        )));
    }
    let prefix = log_probs.narrow(candle_core::D::Minus1, 0, audio_mask_id)?;
    let mask_fill = Tensor::full(
        f32::NEG_INFINITY,
        prefix
            .shape()
            .dims()
            .iter()
            .copied()
            .take(prefix.rank().saturating_sub(1))
            .chain(std::iter::once(1))
            .collect::<Vec<_>>(),
        log_probs.device(),
    )?;
    if audio_mask_id + 1 == vocab_size {
        Tensor::cat(&[&prefix, &mask_fill], candle_core::D::Minus1).map_err(Into::into)
    } else {
        let suffix = log_probs.narrow(
            candle_core::D::Minus1,
            audio_mask_id + 1,
            vocab_size - audio_mask_id - 1,
        )?;
        Tensor::cat(&[&prefix, &mask_fill, &suffix], candle_core::D::Minus1).map_err(Into::into)
    }
}

fn filter_top_k(logits: &Tensor, ratio: f32) -> Result<Tensor> {
    let logits = logits.contiguous()?;
    let vocab_size = logits.dim(candle_core::D::Minus1)?;
    let top_k = ((ratio * vocab_size as f32).ceil() as usize).clamp(1, vocab_size);
    let sorted_indices = logits.arg_sort_last_dim(false)?.contiguous()?;
    let top_indices = sorted_indices
        .narrow(candle_core::D::Minus1, 0, top_k)?
        .contiguous()?;
    let top_values = logits
        .gather(&top_indices, candle_core::D::Minus1)?
        .contiguous()?;
    let masked = Tensor::full(f32::NEG_INFINITY, logits.shape().dims(), logits.device())?;
    masked
        .scatter(&top_indices, &top_values, candle_core::D::Minus1)
        .map_err(Into::into)
}

fn with_rng<T>(
    cpu_rng: Option<&mut StdRng>,
    f: impl FnOnce(&mut StdRng) -> Result<T>,
) -> Result<T> {
    match cpu_rng {
        Some(rng) => f(rng),
        None => {
            let mut seed_src = rand::rng();
            let mut rng = StdRng::from_rng(&mut seed_src);
            f(&mut rng)
        }
    }
}

fn sample_gumbel<R: Rng + ?Sized>(rng: &mut R) -> f32 {
    let uniform = rng.random::<f32>().clamp(1.0e-10, 1.0 - 1.0e-10);
    -(-uniform.ln() + 1.0e-10).ln()
}

fn gumbel_argmax_cpu(
    logits: &Tensor,
    temperature: f32,
    cpu_rng: Option<&mut StdRng>,
) -> Result<Tensor> {
    if temperature <= 0.0 {
        return logits.argmax(candle_core::D::Minus1).map_err(Into::into);
    }
    let (batch, layers, steps, vocab) = logits.dims4()?;
    let device = logits.device().clone();
    let logits = logits
        .to_dtype(DType::F32)?
        .flatten_all()?
        .to_vec1::<f32>()
        .map_err(OmniVoiceError::from)?;
    let mut pred_tokens = Vec::with_capacity(batch * layers * steps);
    with_rng(cpu_rng, |rng| {
        for row in logits.chunks_exact(vocab) {
            let mut best_index = 0usize;
            let mut best_score = f32::NEG_INFINITY;
            for (index, value) in row.iter().enumerate() {
                let score = (*value / temperature) + sample_gumbel(rng);
                if score > best_score {
                    best_score = score;
                    best_index = index;
                }
            }
            pred_tokens.push(best_index as i64);
        }
        Ok(())
    })?;
    Tensor::from_vec(pred_tokens, (batch, layers, steps), &device).map_err(Into::into)
}

fn gumbel_argmax(logits: &Tensor, temperature: f32) -> Result<Tensor> {
    if temperature <= 0.0 {
        return logits.argmax(candle_core::D::Minus1).map_err(Into::into);
    }
    let scores = apply_gumbel_noise(logits, temperature)?;
    scores.argmax(candle_core::D::Minus1).map_err(Into::into)
}

/// Device-side Gumbel noise matching PyTorch OmniVoice `_gumbel_sample`.
///
/// Important: sample U(0,1) in **F32** (not F64). Mixing F64 `rand_like` with
/// F32 logits/eps previously collapsed CUDA unmask schedules into pathological
/// token sequences (listening demo scenario 05: near-DC rumble on CUDA).
fn apply_gumbel_noise(logits: &Tensor, temperature: f32) -> Result<Tensor> {
    let logits = logits.to_dtype(DType::F32)?.contiguous()?;
    let device = logits.device();
    let shape = logits.shape();
    // torch.rand_like(logits) → same dtype/device as logits.
    let uniform = Tensor::rand(0f32, 1f32, shape, device)?.to_dtype(DType::F32)?;
    let eps = 1e-10f32;
    let uniform = uniform
        .maximum(&Tensor::new(eps, device)?.broadcast_as(shape.dims())?)?
        .minimum(&Tensor::new(1.0f32 - eps, device)?.broadcast_as(shape.dims())?)?;
    // gumbel = -log(-log(u))
    let gumbel_noise = uniform.log()?.neg()?.log()?.neg()?;
    let scaled = (&logits / f64::from(temperature))?;
    (scaled + gumbel_noise).map_err(Into::into)
}

fn apply_position_temperature_cpu(
    logits: &Tensor,
    temperature: f32,
    cpu_rng: Option<&mut StdRng>,
) -> Result<Tensor> {
    if temperature <= 0.0 {
        return Ok(logits.clone());
    }
    let shape = logits.shape().dims().to_vec();
    let device = logits.device().clone();
    let logits = logits
        .to_dtype(DType::F32)?
        .flatten_all()?
        .to_vec1::<f32>()
        .map_err(OmniVoiceError::from)?;
    let mut values = Vec::with_capacity(logits.len());
    with_rng(cpu_rng, |rng| {
        for value in logits {
            values.push((value / temperature) + sample_gumbel(rng));
        }
        Ok(())
    })?;
    Tensor::from_vec(values, shape, &device).map_err(Into::into)
}

/// Apply position Gumbel without downloading confidence scores from the GPU.
///
/// Host `StdRng` still owns the noise stream (device `Tensor::rand` Gumbel has
/// historically collapsed unmask schedules). Prefer a pre-uploaded `noise`
/// tensor (one H2D for the whole denoise loop); fall back to per-call sampling.
fn apply_position_temperature_host_noise(
    logits: &Tensor,
    temperature: f32,
    cpu_rng: Option<&mut StdRng>,
    precomputed_noise: Option<&Tensor>,
) -> Result<Tensor> {
    if temperature <= 0.0 {
        return Ok(logits.clone());
    }
    let logits = logits.to_dtype(DType::F32)?;
    let shape = logits.shape().dims().to_vec();
    let n = logits.elem_count();
    let device = logits.device().clone();
    let noise = if let Some(noise) = precomputed_noise {
        noise.to_dtype(DType::F32)?
    } else {
        let mut noise = Vec::with_capacity(n);
        with_rng(cpu_rng, |rng| {
            for _ in 0..n {
                noise.push(sample_gumbel(rng));
            }
            Ok(())
        })?;
        Tensor::from_vec(noise, shape.as_slice(), &device)?
    };
    let scaled = (&logits / f64::from(temperature))?;
    (scaled + noise).map_err(Into::into)
}

/// Pre-sample host Gumbel noise for every denoise step that needs it, upload once.
fn build_position_gumbel_bank(
    schedules: &[Vec<usize>],
    num_codebooks: usize,
    max_target_len: usize,
    num_step: usize,
    device: &Device,
    cpu_rng: &mut StdRng,
) -> Result<Option<Tensor>> {
    let flat = num_codebooks.saturating_mul(max_target_len);
    if flat == 0 {
        return Ok(None);
    }
    let mut values = Vec::new();
    for step in 0..num_step {
        let needs = schedules
            .iter()
            .any(|s| s.get(step).copied().unwrap_or(0) > 0);
        if needs {
            for _ in 0..flat {
                values.push(sample_gumbel(cpu_rng));
            }
        }
    }
    if values.is_empty() {
        return Ok(None);
    }
    let steps_with_noise = values.len() / flat;
    Ok(Some(Tensor::from_vec(
        values,
        (steps_with_noise, 1, num_codebooks, max_target_len),
        device,
    )?))
}

fn validate_batched_inputs(
    batched: &BatchedInputs,
    cond_lens: &[usize],
    target_lens: &[usize],
    expected_codebooks: usize,
    audio_mask_id: i64,
) -> Result<()> {
    let (input_batch, input_codebooks, input_len) = batched.batch_input_ids.dims();
    let (mask_batch, mask_len) = batched.batch_audio_mask.dims();
    let (attention_batch, attention_heads, query_len, key_len) =
        batched.batch_attention_mask.dims();
    let (token_batch, token_codebooks, token_len) = batched.tokens_init.dims();
    let batch_size = cond_lens.len();

    if batch_size == 0 {
        return Err(OmniVoiceError::InvalidRequest(
            "prepared batch must contain at least one item".to_string(),
        ));
    }
    if input_batch != batch_size.saturating_mul(2)
        || input_codebooks != expected_codebooks
        || mask_batch != input_batch
        || mask_len != input_len
        || attention_batch != input_batch
        || attention_heads != 1
        || query_len != input_len
        || key_len != input_len
        || token_batch != batch_size
        || token_codebooks != expected_codebooks
        || token_len == 0
    {
        return Err(OmniVoiceError::InvalidTensorShape {
            name: "stage0_batched_inputs".to_string(),
            expected: format!(
                "input_ids=(2B,{expected_codebooks},S), masks=(2B,S), attention=(2B,1,S,S), tokens=(B,{expected_codebooks},T)"
            ),
            actual: format!(
                "input_ids=({input_batch},{input_codebooks},{input_len}), masks=({mask_batch},{mask_len}), attention=({attention_batch},{attention_heads},{query_len},{key_len}), tokens=({token_batch},{token_codebooks},{token_len})"
            ),
        });
    }

    for (index, (&cond_len, &target_len)) in cond_lens.iter().zip(target_lens).enumerate() {
        if cond_len == 0 || cond_len > input_len {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "conditional length at index {index} must be in 1..={input_len}"
            )));
        }
        if target_len == 0 || target_len > cond_len || target_len > token_len {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "target length at index {index} must be in 1..={cond_len} and fit tokens_init"
            )));
        }
    }
    if batched
        .tokens_init
        .data
        .iter()
        .any(|token| *token != audio_mask_id)
    {
        return Err(OmniVoiceError::InvalidData(
            "tokens_init must be filled with the audio mask token".to_string(),
        ));
    }
    Ok(())
}

fn validate_prepared_batch(
    prepared: &PreparedInferenceBatch,
    expected_codebooks: usize,
    audio_mask_id: i64,
) -> Result<()> {
    let (input_batch, input_codebooks, input_len) = prepared.input_ids.dims3()?;
    let (mask_batch, mask_len) = prepared.audio_mask.dims2()?;
    let (attention_batch, attention_heads, query_len, key_len) = prepared.attention_mask.dims4()?;
    let (token_batch, token_codebooks, token_len) = prepared.tokens_init.dims3()?;
    let batch_size = prepared.target_lens.len();

    if batch_size == 0 || prepared.cond_lens.len() != batch_size {
        return Err(OmniVoiceError::InvalidRequest(
            "prepared batch lengths are inconsistent".to_string(),
        ));
    }
    if input_batch != batch_size.saturating_mul(2)
        || input_codebooks != expected_codebooks
        || mask_batch != input_batch
        || mask_len != input_len
        || attention_batch != input_batch
        || attention_heads != 1
        || query_len != input_len
        || key_len != input_len
        || token_batch != batch_size
        || token_codebooks != expected_codebooks
        || token_len == 0
    {
        return Err(OmniVoiceError::InvalidTensorShape {
            name: "prepared_stage0_batch".to_string(),
            expected: format!(
                "input_ids=(2B,{expected_codebooks},S), masks=(2B,S), attention=(2B,1,S,S), tokens=(B,{expected_codebooks},T)"
            ),
            actual: format!(
                "input_ids=({input_batch},{input_codebooks},{input_len}), masks=({mask_batch},{mask_len}), attention=({attention_batch},{attention_heads},{query_len},{key_len}), tokens=({token_batch},{token_codebooks},{token_len})"
            ),
        });
    }

    for (index, (&cond_len, &target_len)) in prepared
        .cond_lens
        .iter()
        .zip(&prepared.target_lens)
        .enumerate()
    {
        if cond_len == 0 || cond_len > input_len {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "conditional length at index {index} must be in 1..={input_len}"
            )));
        }
        if target_len == 0 || target_len > cond_len || target_len > token_len {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "target length at index {index} must be in 1..={cond_len} and fit tokens_init"
            )));
        }
    }
    // tokens_init is constructed as `Tensor::full(audio_mask_id, ...)` in the
    // pack path. Skip a device→host sync check on every generate (was a hidden
    // tax on short CUDA runs). Shape/length checks above still apply.
    let _ = audio_mask_id;
    Ok(())
}

fn validate_deterministic_config(config: &Stage0DeterministicConfig) -> Result<()> {
    if config.num_step == 0 || config.num_step > MAX_GENERATION_STEPS {
        return Err(OmniVoiceError::InvalidRequest(format!(
            "num_step must be in the range 1..={MAX_GENERATION_STEPS}"
        )));
    }
    if !config.guidance_scale.is_finite() {
        return Err(OmniVoiceError::InvalidRequest(
            "guidance_scale must be finite".to_string(),
        ));
    }
    if !config.t_shift.is_finite() || config.t_shift <= 0.0 {
        return Err(OmniVoiceError::InvalidRequest(
            "t_shift must be finite and greater than zero".to_string(),
        ));
    }
    if !config.layer_penalty_factor.is_finite() {
        return Err(OmniVoiceError::InvalidRequest(
            "layer_penalty_factor must be finite".to_string(),
        ));
    }
    if !config.position_temperature.is_finite() || config.position_temperature < 0.0 {
        return Err(OmniVoiceError::InvalidRequest(
            "position_temperature must be finite and non-negative".to_string(),
        ));
    }
    if !config.class_temperature.is_finite() || config.class_temperature < 0.0 {
        return Err(OmniVoiceError::InvalidRequest(
            "class_temperature must be finite and non-negative".to_string(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn apply_step_updates_device(
    current_tokens: &Tensor,
    predicted_tokens: &Tensor,
    confidence_scores: &Tensor,
    mask_id: i64,
    update_count: usize,
    layer_penalties: &Tensor,
    position_temperature: f32,
    cpu_rng: Option<&mut StdRng>,
    precomputed_noise: Option<&Tensor>,
) -> Result<Tensor> {
    if update_count == 0 {
        return Ok(current_tokens.clone());
    }
    let selection_scores = confidence_scores
        .broadcast_sub(&layer_penalties.broadcast_as(confidence_scores.shape().dims())?)?;
    // Host StdRng for Gumbel draws (quality); apply noise on-device to avoid
    // per-step D2H of confidence scores that serializes the CUDA pipeline.
    let selection_scores = if position_temperature > 0.0 {
        if selection_scores.device().is_cpu() {
            apply_position_temperature_cpu(&selection_scores, position_temperature, cpu_rng)?
        } else {
            apply_position_temperature_host_noise(
                &selection_scores,
                position_temperature,
                cpu_rng,
                precomputed_noise,
            )?
        }
    } else {
        selection_scores
    };
    let available_mask = current_tokens.eq(mask_id)?;
    let neg_inf = Tensor::full(
        f32::NEG_INFINITY,
        selection_scores.shape().dims(),
        current_tokens.device(),
    )?;
    let masked_scores = available_mask.where_cond(&selection_scores, &neg_inf)?;
    let flat_scores = masked_scores.flatten_all()?;
    let flat_len = flat_scores.elem_count();
    let top_k = update_count.min(flat_len);
    let sorted_indices = flat_scores
        .reshape((1, flat_len))?
        .arg_sort_last_dim(false)?;
    let top_indices = sorted_indices.i((0, 0..top_k))?;
    let flat_predicted = predicted_tokens.flatten_all()?;
    let update_values = flat_predicted.gather(&top_indices, 0)?;
    let flat_current = current_tokens.flatten_all()?;
    let updated = if current_tokens.device().is_metal() && current_tokens.dtype() == DType::I64 {
        let flat_current = flat_current.to_dtype(DType::U32)?;
        let update_values = update_values.to_dtype(DType::U32)?;
        flat_current
            .scatter(&top_indices, &update_values, 0)?
            .to_dtype(DType::I64)?
    } else {
        flat_current.scatter(&top_indices, &update_values, 0)?
    };
    updated
        .reshape(current_tokens.shape().dims())
        .map_err(Into::into)
}

fn tensor_parity_metric(actual: &[f32], expected: &[f32]) -> Result<Stage0ParityMetric> {
    if actual.len() != expected.len() {
        return Err(OmniVoiceError::InvalidData(format!(
            "tensor parity length mismatch: actual len {} != expected len {}",
            actual.len(),
            expected.len()
        )));
    }
    let mut max_abs = 0.0_f32;
    let mut abs_sum = 0.0_f64;
    let mut squared_sum = 0.0_f64;
    let mut exact_match = true;
    for (lhs, rhs) in actual.iter().zip(expected.iter()) {
        let diff = *lhs - *rhs;
        if diff != 0.0 {
            exact_match = false;
        }
        max_abs = max_abs.max(diff.abs());
        abs_sum += f64::from(diff.abs());
        squared_sum += f64::from(diff * diff);
    }
    let count = actual.len().max(1) as f64;
    Ok(Stage0ParityMetric {
        exact_match,
        max_abs,
        mae: (abs_sum / count) as f32,
        rmse: (squared_sum / count).sqrt() as f32,
    })
}

fn exact_i64_parity_metric(actual: &[i64], expected: &[i64]) -> Stage0ParityMetric {
    Stage0ParityMetric {
        exact_match: actual == expected,
        max_abs: if actual == expected {
            0.0
        } else {
            f32::INFINITY
        },
        mae: if actual == expected {
            0.0
        } else {
            f32::INFINITY
        },
        rmse: if actual == expected {
            0.0
        } else {
            f32::INFINITY
        },
    }
}

impl Stage0ParityMetrics {
    pub fn from_debug_capture(
        actual: &Stage0DebugCapture,
        reference_forward: &crate::artifacts::ForwardStepZero,
        reference_steps: &[(usize, crate::artifacts::StepCapture)],
        reference_final_tokens: &I64Tensor2,
    ) -> Result<Self> {
        let mut metrics = BTreeMap::new();
        metrics.insert(
            "inputs_embeds".to_string(),
            tensor_parity_metric(
                &actual.inputs_embeds.data,
                &reference_forward.inputs_embeds.data,
            )?,
        );
        for (layer, reference_hidden) in &reference_forward.hidden_layers {
            let actual_hidden = actual.hidden_layers.get(layer).ok_or_else(|| {
                OmniVoiceError::InvalidData(format!(
                    "missing actual hidden layer {:02} capture",
                    layer
                ))
            })?;
            metrics.insert(
                format!("hidden_layer_{layer:02}"),
                tensor_parity_metric(&actual_hidden.data, &reference_hidden.data)?,
            );
        }
        metrics.insert(
            "final_hidden".to_string(),
            tensor_parity_metric(
                &actual.final_hidden.data,
                &reference_forward.final_hidden.data,
            )?,
        );
        metrics.insert(
            "final_tokens".to_string(),
            exact_i64_parity_metric(&actual.final_tokens.data, &reference_final_tokens.data),
        );
        for (step, reference) in reference_steps {
            let actual_step = actual
                .steps
                .iter()
                .find(|capture| capture.step == *step)
                .ok_or_else(|| {
                    OmniVoiceError::InvalidData(format!(
                        "missing actual stage0 debug capture for step {step}"
                    ))
                })?;
            metrics.insert(
                format!("step_{step:02}_c_logits"),
                tensor_parity_metric(&actual_step.c_logits.data, &reference.c_logits.data)?,
            );
            metrics.insert(
                format!("step_{step:02}_u_logits"),
                tensor_parity_metric(&actual_step.u_logits.data, &reference.u_logits.data)?,
            );
            metrics.insert(
                format!("step_{step:02}_pred_tokens"),
                exact_i64_parity_metric(&actual_step.pred_tokens.data, &reference.pred_tokens.data),
            );
            metrics.insert(
                format!("step_{step:02}_confidence_scores"),
                tensor_parity_metric(
                    &actual_step.confidence_scores.data,
                    &reference.confidence_scores.data,
                )?,
            );
            metrics.insert(
                format!("step_{step:02}_batch_input_ids_before_step"),
                exact_i64_parity_metric(
                    &actual_step.batch_input_ids_before_step.data,
                    &reference.batch_input_ids_before_step.data,
                ),
            );
            metrics.insert(
                format!("step_{step:02}_tokens_after_step"),
                exact_i64_parity_metric(
                    &actual_step.tokens_after_step.data,
                    &reference.tokens_after_step.data,
                ),
            );
        }
        Ok(Self { metrics })
    }

    pub fn insert_exact_i64_metric(
        &mut self,
        name: impl Into<String>,
        actual: &[i64],
        expected: &[i64],
    ) {
        self.metrics
            .insert(name.into(), exact_i64_parity_metric(actual, expected));
    }
}

fn mmap_var_builder(
    weights_path: &Path,
    dtype: DType,
    device: &Device,
) -> Result<VarBuilder<'static>> {
    let paths = [weights_path];
    // SAFETY: Candle exposes mmap loading behind an unsafe API because it wraps OS-backed
    // read-only memory maps. We map immutable safetensors files and only hand the backend to
    // Candle for read-only tensor materialization.
    Ok(unsafe { VarBuilder::from_mmaped_safetensors(&paths, dtype, device)? })
}

fn gguf_metadata_usize(
    content: &candle_core::quantized::gguf_file::Content,
    key: &str,
) -> Result<usize> {
    match content.metadata.get(key) {
        Some(candle_core::quantized::gguf_file::Value::U8(v)) => Ok((*v).into()),
        Some(candle_core::quantized::gguf_file::Value::I8(v)) if *v >= 0 => Ok(*v as usize),
        Some(candle_core::quantized::gguf_file::Value::U16(v)) => Ok((*v).into()),
        Some(candle_core::quantized::gguf_file::Value::I16(v)) if *v >= 0 => Ok(*v as usize),
        Some(candle_core::quantized::gguf_file::Value::U32(v)) => Ok(*v as usize),
        Some(candle_core::quantized::gguf_file::Value::I32(v)) if *v >= 0 => Ok(*v as usize),
        Some(candle_core::quantized::gguf_file::Value::U64(v)) => {
            usize::try_from(*v).map_err(|_| {
                OmniVoiceError::InvalidData(format!("GGUF metadata `{key}` exceeds usize"))
            })
        }
        Some(candle_core::quantized::gguf_file::Value::I64(v)) if *v >= 0 => usize::try_from(*v)
            .map_err(|_| {
                OmniVoiceError::InvalidData(format!("GGUF metadata `{key}` exceeds usize"))
            }),
        Some(other) => Err(OmniVoiceError::InvalidData(format!(
            "GGUF metadata `{key}` is not a non-negative integer: {other:?}"
        ))),
        None => Err(OmniVoiceError::InvalidData(format!(
            "missing GGUF metadata key `{key}`"
        ))),
    }
}

fn gguf_metadata_f64(
    content: &candle_core::quantized::gguf_file::Content,
    key: &str,
) -> Result<f64> {
    match content.metadata.get(key) {
        Some(candle_core::quantized::gguf_file::Value::F32(v)) => Ok((*v).into()),
        Some(candle_core::quantized::gguf_file::Value::F64(v)) => Ok(*v),
        Some(other) => Err(OmniVoiceError::InvalidData(format!(
            "GGUF metadata `{key}` is not a float: {other:?}"
        ))),
        None => Err(OmniVoiceError::InvalidData(format!(
            "missing GGUF metadata key `{key}`"
        ))),
    }
}

fn tensor_to_f32_tensor3(tensor: &Tensor) -> Result<F32Tensor3> {
    let dims = tensor.dims3()?;
    let data = tensor
        .to_device(&Device::Cpu)?
        .to_dtype(DType::F32)?
        .flatten_all()?
        .to_vec1::<f32>()?;
    F32Tensor3::new(dims, data)
}

fn tensor_to_f32_tensor4(tensor: &Tensor) -> Result<F32Tensor4> {
    let dims = tensor.dims4()?;
    let data = tensor
        .to_device(&Device::Cpu)?
        .to_dtype(DType::F32)?
        .flatten_all()?
        .to_vec1::<f32>()?;
    F32Tensor4::new(dims, data)
}

fn tensor_to_i64_tensor3(tensor: &Tensor) -> Result<I64Tensor3> {
    let dims = tensor.dims3()?;
    let data = tensor
        .to_device(&Device::Cpu)?
        .to_dtype(DType::I64)?
        .flatten_all()?
        .to_vec1::<i64>()?;
    I64Tensor3::new(dims, data)
}

pub(crate) fn tensor_to_i64_tensor2(tensor: &Tensor) -> Result<I64Tensor2> {
    let dims = tensor.dims2()?;
    let data = tensor
        .to_device(&Device::Cpu)?
        .to_dtype(DType::I64)?
        .flatten_all()?
        .to_vec1::<i64>()?;
    I64Tensor2::new(dims, data)
}
