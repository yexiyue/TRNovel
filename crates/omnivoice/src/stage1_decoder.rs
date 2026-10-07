use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::OnceLock,
};

use candle_core::{DType, Device, Tensor, quantized::gguf_file};
use regex::Regex;
use serde::Deserialize;

use crate::{
    artifacts::{AudioTokenizerArtifacts, GgufBundleArtifacts, RuntimeArtifacts, RuntimeContracts},
    codec_layers::CodecWeightSource,
    contracts::{DecodedAudio, F32Tensor3, GeneratedTokens, I64Tensor2},
    error::{OmniVoiceError, Result},
    postprocess::{
        apply_clone_rms_restore, cross_fade_chunks, ensure_non_empty_audio, fade_and_pad_audio,
        peak_normalize_auto_voice, remove_silence,
    },
    runtime::RuntimeOptions,
    stage1_model::{Stage1DecodeTrace, Stage1Model, Stage1ModelConfig},
};

#[derive(Debug, Clone)]
pub struct Stage1DecoderBundle {
    model_root: PathBuf,
    weights_path: PathBuf,
    output_sample_rate: u32,
    expected_codebooks: usize,
    model_config: Stage1ModelConfig,
}

#[derive(Debug)]
pub struct PreparedStage1Decode {
    pub tokens: Tensor,
    pub sample_rate: u32,
    pub hop_length: usize,
    pub frame_rate: usize,
    pub expected_codebooks: usize,
    pub ref_rms: Option<f32>,
    pub runtime_dtype: DType,
}

impl PreparedStage1Decode {
    pub fn token_dims(&self) -> Result<(usize, usize, usize)> {
        let dims = self.tokens.dims();
        if let [a, b, c] = dims {
            Ok((*a, *b, *c))
        } else {
            Err(OmniVoiceError::InvalidTensorShape {
                name: "stage1_tokens".to_string(),
                expected: "(1, C, T)".to_string(),
                actual: format!("{dims:?}"),
            })
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage1TensorStats {
    pub min: f32,
    pub max: f32,
    pub mean: f32,
    pub std: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage1TensorParityMetric {
    pub element_count: usize,
    pub cosine_similarity: f32,
    pub max_abs: f32,
    pub mae: f32,
    pub rmse: f32,
    pub actual: Stage1TensorStats,
    pub reference: Stage1TensorStats,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage1DebugCapture {
    pub tensors: BTreeMap<String, F32Tensor3>,
    pub raw_audio: DecodedAudio,
    pub final_audio: DecodedAudio,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Stage1DebugRun {
    pub tensor_metrics: BTreeMap<String, Stage1TensorParityMetric>,
    pub raw_audio_metrics: crate::contracts::AudioParityMetrics,
    pub final_audio_metrics: crate::contracts::AudioParityMetrics,
    pub debug_capture: Stage1DebugCapture,
}

#[derive(Debug)]
pub struct Stage1RuntimePlan {
    options: RuntimeOptions,
    bundle: Stage1DecoderBundle,
    device: Device,
    runtime_dtype: DType,
    hop_length: usize,
    frame_rate: usize,
    model: OnceLock<std::result::Result<Stage1Model, String>>,
}

#[derive(Debug, Deserialize)]
struct MainModelConfig {
    num_audio_codebook: usize,
}

#[derive(Debug, Deserialize)]
struct AudioTokenizerConfig {
    sample_rate: u32,
    downsample_factor: usize,
}

#[derive(Debug, Deserialize)]
struct AudioTokenizerPreprocessorConfig {
    sampling_rate: u32,
    hop_length: usize,
}

impl Stage1DecoderBundle {
    pub fn from_model_root(model_root: impl AsRef<Path>) -> Result<Self> {
        let runtime = RuntimeArtifacts::from_model_root(model_root)?;
        Self::from_runtime_artifacts(&runtime)
    }

    pub fn from_runtime_artifacts(runtime: &RuntimeArtifacts) -> Result<Self> {
        if let Some(gguf) = runtime.gguf_bundle() {
            return Self::from_gguf_bundle(runtime.model_root(), gguf, runtime.contracts());
        }
        Self::from_artifacts(
            runtime.model_root(),
            runtime.audio_tokenizer()?,
            runtime.contracts(),
        )
    }

    fn from_gguf_bundle(
        model_root: impl AsRef<Path>,
        gguf: &GgufBundleArtifacts,
        contracts: &RuntimeContracts,
    ) -> Result<Self> {
        let model_root = model_root.as_ref().to_path_buf();
        let expected_codebooks = contracts.num_audio_codebooks;
        let model_config =
            inspect_stage1_model_config(gguf.audio_tokenizer_path(), expected_codebooks)?;
        let expected_audio_vocab_size =
            model_config.codebook_size.checked_add(1).ok_or_else(|| {
                OmniVoiceError::InvalidData(
                    "stage1 codebook size overflows audio vocabulary".to_string(),
                )
            })?;
        if expected_audio_vocab_size != contracts.audio_vocab_size {
            return Err(OmniVoiceError::InvalidData(format!(
                "stage1 codebook size {} does not match manifest audio_vocab_size {}",
                model_config.codebook_size, contracts.audio_vocab_size
            )));
        }
        Ok(Self {
            model_root,
            weights_path: gguf.audio_tokenizer_path().to_path_buf(),
            output_sample_rate: contracts.sample_rate,
            expected_codebooks,
            model_config,
        })
    }

    pub fn from_artifacts(
        model_root: impl AsRef<Path>,
        audio_tokenizer: &AudioTokenizerArtifacts,
        contracts: &RuntimeContracts,
    ) -> Result<Self> {
        let model_root = model_root.as_ref().to_path_buf();
        let main_model: MainModelConfig =
            serde_json::from_str(&fs::read_to_string(model_root.join("config.json"))?)?;
        let tokenizer_model: AudioTokenizerConfig =
            serde_json::from_str(&fs::read_to_string(audio_tokenizer.config_path())?)?;
        let preprocessor: AudioTokenizerPreprocessorConfig = serde_json::from_str(
            &fs::read_to_string(audio_tokenizer.preprocessor_config_path())?,
        )?;

        if tokenizer_model.sample_rate != contracts.sample_rate
            || preprocessor.sampling_rate != contracts.sample_rate
        {
            return Err(OmniVoiceError::InvalidData(format!(
                "stage1 sample rate mismatch: tokenizer={}, preprocessor={}, manifest={}",
                tokenizer_model.sample_rate, preprocessor.sampling_rate, contracts.sample_rate
            )));
        }
        if preprocessor.hop_length != contracts.hop_length {
            return Err(OmniVoiceError::InvalidData(format!(
                "stage1 hop_length mismatch: preprocessor={}, manifest={}",
                preprocessor.hop_length, contracts.hop_length
            )));
        }
        if tokenizer_model.downsample_factor == 0 {
            return Err(OmniVoiceError::InvalidData(
                "audio tokenizer downsample_factor must be > 0".to_string(),
            ));
        }
        if main_model.num_audio_codebook != contracts.num_audio_codebooks {
            return Err(OmniVoiceError::InvalidData(format!(
                "stage1 expected codebooks mismatch: main model={}, manifest={}",
                main_model.num_audio_codebook, contracts.num_audio_codebooks
            )));
        }

        let model_config = inspect_stage1_model_config(
            audio_tokenizer.weights_path(),
            contracts.num_audio_codebooks,
        )?;
        let expected_audio_vocab_size =
            model_config.codebook_size.checked_add(1).ok_or_else(|| {
                OmniVoiceError::InvalidData(
                    "stage1 codebook size overflows audio vocabulary".to_string(),
                )
            })?;
        if expected_audio_vocab_size != contracts.audio_vocab_size {
            return Err(OmniVoiceError::InvalidData(format!(
                "stage1 codebook size {} does not match manifest audio_vocab_size {}",
                model_config.codebook_size, contracts.audio_vocab_size
            )));
        }
        if contracts.token_id_min < 0
            || contracts.token_id_max < contracts.token_id_min
            || contracts.token_id_max >= model_config.codebook_size as i64
        {
            return Err(OmniVoiceError::InvalidData(format!(
                "manifest token range {}..={} is outside stage1 codebook size {}",
                contracts.token_id_min, contracts.token_id_max, model_config.codebook_size
            )));
        }

        Ok(Self {
            model_root,
            weights_path: audio_tokenizer.weights_path().to_path_buf(),
            output_sample_rate: tokenizer_model.sample_rate,
            expected_codebooks: contracts.num_audio_codebooks,
            model_config,
        })
    }

    pub fn weights_path(&self) -> &Path {
        &self.weights_path
    }

    pub fn output_sample_rate(&self) -> u32 {
        self.output_sample_rate
    }

    pub fn expected_codebooks(&self) -> usize {
        self.expected_codebooks
    }

    pub fn active_quantizer_indices(&self) -> &[usize] {
        &self.model_config.active_quantizer_indices
    }

    pub fn model_config(&self) -> &Stage1ModelConfig {
        &self.model_config
    }

    pub fn validate_generated_tokens(&self, tokens: &I64Tensor2) -> Result<()> {
        let (layers, positions) = tokens.dims();
        if layers != self.expected_codebooks {
            return Err(OmniVoiceError::InvalidTensorShape {
                name: "generated_tokens".to_string(),
                expected: format!("({}, T)", self.expected_codebooks),
                actual: format!("({}, {})", layers, positions),
            });
        }
        if positions == 0 {
            return Err(OmniVoiceError::InvalidRequest(
                "generated tokens must contain at least one frame".to_string(),
            ));
        }
        if tokens
            .data
            .iter()
            .any(|token| *token < 0 || *token >= self.model_config.codebook_size as i64)
        {
            return Err(OmniVoiceError::InvalidData(format!(
                "generated tokens must stay within [0, {}]",
                self.model_config.codebook_size.saturating_sub(1)
            )));
        }
        Ok(())
    }

    pub fn model_root(&self) -> &Path {
        &self.model_root
    }
}

impl Stage1RuntimePlan {
    pub fn from_options(options: RuntimeOptions) -> Result<Self> {
        let runtime = options.load_runtime_artifacts()?;
        Self::from_runtime_artifacts(options, &runtime)
    }

    pub fn from_runtime_artifacts(
        options: RuntimeOptions,
        runtime: &RuntimeArtifacts,
    ) -> Result<Self> {
        let bundle = Stage1DecoderBundle::from_runtime_artifacts(runtime)?;
        let device = options.resolve_device()?;
        Self::from_runtime_artifacts_with_device(options, runtime, bundle, device)
    }

    pub fn from_runtime_artifacts_with_device(
        options: RuntimeOptions,
        runtime: &RuntimeArtifacts,
        bundle: Stage1DecoderBundle,
        device: Device,
    ) -> Result<Self> {
        let runtime_dtype = options.resolve_audio_dtype_for_runtime_device(&device);

        Ok(Self {
            bundle,
            device,
            runtime_dtype,
            hop_length: runtime.contracts().hop_length,
            frame_rate: runtime.contracts().frame_rate,
            options,
            model: OnceLock::new(),
        })
    }

    pub fn prepare_decode(
        &self,
        tokens: &I64Tensor2,
        ref_rms: Option<f32>,
    ) -> Result<PreparedStage1Decode> {
        self.bundle.validate_generated_tokens(tokens)?;
        validate_ref_rms(ref_rms)?;
        let tensor = tokens.to_candle(&self.device)?.unsqueeze(0)?;

        Ok(PreparedStage1Decode {
            tokens: tensor,
            sample_rate: self.bundle.output_sample_rate(),
            hop_length: self.hop_length,
            frame_rate: self.frame_rate,
            expected_codebooks: self.bundle.expected_codebooks(),
            ref_rms,
            runtime_dtype: self.runtime_dtype,
        })
    }

    pub fn decode_raw(
        &self,
        tokens: &GeneratedTokens,
        ref_rms: Option<f32>,
    ) -> Result<DecodedAudio> {
        validate_ref_rms(ref_rms)?;
        let samples = match tokens {
            GeneratedTokens::Single(tokens) => self.decode_chunk(tokens)?,
            GeneratedTokens::Chunked(chunks) => {
                let decoded = chunks
                    .iter()
                    .map(|chunk| self.decode_chunk(chunk))
                    .collect::<Result<Vec<_>>>()?;
                cross_fade_chunks(&decoded, self.bundle.output_sample_rate(), 0.3)?
            }
        };
        let audio = DecodedAudio::new(samples, self.bundle.output_sample_rate());
        audio.validate()?;
        Ok(audio)
    }

    pub fn decode_final(
        &self,
        tokens: &GeneratedTokens,
        ref_rms: Option<f32>,
        postprocess_output: bool,
    ) -> Result<DecodedAudio> {
        validate_ref_rms(ref_rms)?;
        let mut audio = self.decode_raw(tokens, ref_rms)?;
        if postprocess_output {
            let trimmed = remove_silence(&audio.samples, audio.sample_rate, 500, 100, 100);
            if !trimmed.is_empty() {
                audio.samples = trimmed;
            }
        }
        ensure_non_empty_audio(&audio.samples)?;
        audio.samples = if let Some(rms) = ref_rms {
            apply_clone_rms_restore(&audio.samples, rms)
        } else {
            peak_normalize_auto_voice(&audio.samples)?
        };
        audio.samples = fade_and_pad_audio(&audio.samples, audio.sample_rate, 0.1, 0.1);
        audio.validate()?;
        Ok(audio)
    }

    pub fn debug_decode(
        &self,
        tokens: &GeneratedTokens,
        ref_rms: Option<f32>,
        postprocess_output: bool,
    ) -> Result<Stage1DebugCapture> {
        validate_ref_rms(ref_rms)?;
        let mut tensors = BTreeMap::new();
        let raw_audio = match tokens {
            GeneratedTokens::Single(tokens) => {
                let (chunk_tensors, chunk_samples) = self.debug_decode_chunk(tokens)?;
                tensors.extend(chunk_tensors);
                DecodedAudio::new(chunk_samples, self.bundle.output_sample_rate())
            }
            GeneratedTokens::Chunked(chunks) => {
                let mut decoded = Vec::with_capacity(chunks.len());
                for (index, chunk) in chunks.iter().enumerate() {
                    let (chunk_tensors, chunk_samples) = self.debug_decode_chunk(chunk)?;
                    for (name, tensor) in chunk_tensors {
                        tensors.insert(format!("chunk_{index:02}_{name}"), tensor);
                    }
                    decoded.push(chunk_samples);
                }
                DecodedAudio::new(
                    cross_fade_chunks(&decoded, self.bundle.output_sample_rate(), 0.3)?,
                    self.bundle.output_sample_rate(),
                )
            }
        };
        raw_audio.validate()?;

        tensors.insert(
            "raw_waveform".to_string(),
            audio_to_tensor3(&raw_audio.samples)?,
        );
        let mut final_audio = raw_audio.clone();
        if postprocess_output {
            let trimmed =
                remove_silence(&final_audio.samples, final_audio.sample_rate, 500, 100, 100);
            if !trimmed.is_empty() {
                final_audio.samples = trimmed;
            }
        }
        ensure_non_empty_audio(&final_audio.samples)?;
        final_audio.samples = if let Some(rms) = ref_rms {
            apply_clone_rms_restore(&final_audio.samples, rms)
        } else {
            peak_normalize_auto_voice(&final_audio.samples)?
        };
        final_audio.samples =
            fade_and_pad_audio(&final_audio.samples, final_audio.sample_rate, 0.1, 0.1);
        final_audio.validate()?;
        tensors.insert(
            "final_waveform".to_string(),
            audio_to_tensor3(&final_audio.samples)?,
        );

        Ok(Stage1DebugCapture {
            tensors,
            raw_audio,
            final_audio,
        })
    }

    pub fn bundle(&self) -> &Stage1DecoderBundle {
        &self.bundle
    }

    pub fn options(&self) -> &RuntimeOptions {
        &self.options
    }

    pub fn is_loaded(&self) -> bool {
        self.model.get().is_some()
    }

    pub fn decode_final_tensor(
        &self,
        tokens: &Tensor,
        ref_rms: Option<f32>,
        postprocess_output: bool,
    ) -> Result<DecodedAudio> {
        validate_ref_rms(ref_rms)?;
        let mut audio = self.decode_raw_tensor(tokens)?;
        if postprocess_output {
            let trimmed = remove_silence(&audio.samples, audio.sample_rate, 500, 100, 100);
            if !trimmed.is_empty() {
                audio.samples = trimmed;
            }
        }
        ensure_non_empty_audio(&audio.samples)?;
        audio.samples = if let Some(rms) = ref_rms {
            apply_clone_rms_restore(&audio.samples, rms)
        } else {
            peak_normalize_auto_voice(&audio.samples)?
        };
        audio.samples = fade_and_pad_audio(&audio.samples, audio.sample_rate, 0.1, 0.1);
        audio.validate()?;
        Ok(audio)
    }

    pub fn decode_final_tensor_chunks(
        &self,
        chunks: &[Tensor],
        ref_rms: Option<f32>,
        postprocess_output: bool,
    ) -> Result<DecodedAudio> {
        validate_ref_rms(ref_rms)?;
        let mut audio = self.decode_raw_tensor_chunks(chunks)?;
        if postprocess_output {
            let trimmed = remove_silence(&audio.samples, audio.sample_rate, 500, 100, 100);
            if !trimmed.is_empty() {
                audio.samples = trimmed;
            }
        }
        ensure_non_empty_audio(&audio.samples)?;
        audio.samples = if let Some(rms) = ref_rms {
            apply_clone_rms_restore(&audio.samples, rms)
        } else {
            peak_normalize_auto_voice(&audio.samples)?
        };
        audio.samples = fade_and_pad_audio(&audio.samples, audio.sample_rate, 0.1, 0.1);
        audio.validate()?;
        Ok(audio)
    }

    fn decode_chunk(&self, tokens: &I64Tensor2) -> Result<Vec<f32>> {
        let tokens = tokens.to_candle(&self.device)?.unsqueeze(0)?;
        self.decode_audio_tensor(&tokens)
    }

    fn debug_decode_chunk(
        &self,
        tokens: &I64Tensor2,
    ) -> Result<(BTreeMap<String, F32Tensor3>, Vec<f32>)> {
        self.bundle.validate_generated_tokens(tokens)?;
        let trace = self
            .model()?
            .decode_tokens_with_trace(tokens, &self.device)?;
        let decoded = trace.raw_waveform.clone();
        let dims = decoded.dims();
        match dims {
            [1, 1, _] => {}
            _ => {
                return Err(OmniVoiceError::InvalidTensorShape {
                    name: "stage1_decoded_waveform".to_string(),
                    expected: "(1, 1, T)".to_string(),
                    actual: format!("{dims:?}"),
                });
            }
        }
        let mut tensors = trace_to_tensor_map(&trace)?;
        let raw_samples = waveform_to_samples(&decoded)?;
        tensors.insert("raw_waveform".to_string(), audio_to_tensor3(&raw_samples)?);
        Ok((tensors, raw_samples))
    }

    fn decode_raw_tensor(&self, tokens: &Tensor) -> Result<DecodedAudio> {
        Ok(DecodedAudio::new(
            self.decode_audio_tensor(tokens)?,
            self.bundle.output_sample_rate(),
        ))
    }

    fn decode_raw_tensor_chunks(&self, chunks: &[Tensor]) -> Result<DecodedAudio> {
        let decoded = chunks
            .iter()
            .map(|chunk| self.decode_audio_tensor(chunk))
            .collect::<Result<Vec<_>>>()?;
        Ok(DecodedAudio::new(
            cross_fade_chunks(&decoded, self.bundle.output_sample_rate(), 0.3)?,
            self.bundle.output_sample_rate(),
        ))
    }

    fn decode_audio_tensor(&self, tokens: &Tensor) -> Result<Vec<f32>> {
        let tokens = ensure_stage1_tensor(tokens, &self.device)?;
        let decoded = self.model()?.decode_tensor(&tokens)?;
        waveform_to_samples(&decoded)
    }

    fn model(&self) -> Result<&Stage1Model> {
        let result = self.model.get_or_init(|| {
            CodecWeightSource::from_weight_path(
                self.bundle.weights_path(),
                self.runtime_dtype,
                &self.device,
            )
            .and_then(|ws| Stage1Model::load(&ws, self.bundle.model_config()))
            .map_err(|error| error.to_string())
        });
        match result {
            Ok(model) => Ok(model),
            Err(message) => Err(OmniVoiceError::InvalidData(message.clone())),
        }
    }
}

fn trace_to_tensor_map(trace: &Stage1DecodeTrace) -> Result<BTreeMap<String, F32Tensor3>> {
    let mut tensors = BTreeMap::new();
    tensors.insert(
        "project_out".to_string(),
        tensor_to_f32_tensor3(&trace.project_out)?,
    );
    tensors.insert(
        "quantizer_output".to_string(),
        tensor_to_f32_tensor3(&trace.quantizer_output)?,
    );
    tensors.insert(
        "fc2_output".to_string(),
        tensor_to_f32_tensor3(&trace.fc2_output)?,
    );
    tensors.insert(
        "decoder_input".to_string(),
        tensor_to_f32_tensor3(&trace.decoder_input)?,
    );
    for (index, block_output) in trace.decoder_block_outputs.iter().enumerate() {
        tensors.insert(
            format!("decoder_block_{index:02}"),
            tensor_to_f32_tensor3(block_output)?,
        );
    }
    Ok(tensors)
}

fn audio_to_tensor3(samples: &[f32]) -> Result<F32Tensor3> {
    F32Tensor3::new((1, 1, samples.len()), samples.to_vec())
}

fn validate_ref_rms(ref_rms: Option<f32>) -> Result<()> {
    if let Some(value) = ref_rms
        && (!value.is_finite() || value < 0.0)
    {
        return Err(OmniVoiceError::InvalidRequest(
            "reference RMS must be finite and non-negative".to_string(),
        ));
    }
    Ok(())
}

fn ensure_stage1_tensor(tokens: &Tensor, device: &Device) -> Result<Tensor> {
    let dims = tokens.dims();
    match dims {
        [_, _] => {
            return tokens
                .to_device(device)?
                .unsqueeze(0)?
                .contiguous()
                .map_err(Into::into);
        }
        [1, _, _] => {}
        _ => {
            return Err(OmniVoiceError::InvalidTensorShape {
                name: "stage1_tokens".to_string(),
                expected: "(C, T) or (1, C, T)".to_string(),
                actual: format!("{dims:?}"),
            });
        }
    }
    tokens.to_device(device)?.contiguous().map_err(Into::into)
}

fn waveform_to_samples(tensor: &Tensor) -> Result<Vec<f32>> {
    let decoded = tensor.to_device(&Device::Cpu)?;
    let dims = decoded.dims();
    match dims {
        [1, 1, _] => {}
        _ => {
            return Err(OmniVoiceError::InvalidTensorShape {
                name: "stage1_decoded_waveform".to_string(),
                expected: "(1, 1, T)".to_string(),
                actual: format!("{dims:?}"),
            });
        }
    }
    let samples = decoded
        .flatten_all()?
        .to_dtype(DType::F32)?
        .to_vec1::<f32>()
        .map_err(OmniVoiceError::from)?;
    if let Some(index) = samples.iter().position(|sample| !sample.is_finite()) {
        return Err(OmniVoiceError::InvalidData(format!(
            "stage1 decoder produced a non-finite sample at index {index}"
        )));
    }
    Ok(samples)
}

fn tensor_to_f32_tensor3(tensor: &Tensor) -> Result<F32Tensor3> {
    let tensor = tensor.to_device(&Device::Cpu)?.to_dtype(DType::F32)?;
    let dims = tensor.dims();
    match dims {
        [a, b, c] => F32Tensor3::new((*a, *b, *c), tensor.flatten_all()?.to_vec1::<f32>()?),
        _ => Err(OmniVoiceError::InvalidTensorShape {
            name: "stage1_debug_tensor".to_string(),
            expected: "(A, B, C)".to_string(),
            actual: format!("{dims:?}"),
        }),
    }
}

pub(crate) fn stage1_tensor_metrics(
    actual: &BTreeMap<String, F32Tensor3>,
    reference: &BTreeMap<String, F32Tensor3>,
) -> Result<BTreeMap<String, Stage1TensorParityMetric>> {
    let mut metrics = BTreeMap::new();
    for (name, expected) in reference {
        let Some(actual_tensor) = actual.get(name) else {
            return Err(OmniVoiceError::InvalidData(format!(
                "missing actual stage1 debug tensor {name}"
            )));
        };
        metrics.insert(
            name.clone(),
            stage1_tensor_parity_metric(&actual_tensor.data, &expected.data)?,
        );
    }
    Ok(metrics)
}

fn stage1_tensor_parity_metric(
    actual: &[f32],
    reference: &[f32],
) -> Result<Stage1TensorParityMetric> {
    if actual.len() != reference.len() {
        return Err(OmniVoiceError::InvalidData(format!(
            "stage1 tensor parity length mismatch: actual len {} != expected len {}",
            actual.len(),
            reference.len()
        )));
    }
    let actual_stats = stage1_tensor_stats(actual);
    let reference_stats = stage1_tensor_stats(reference);
    if actual.is_empty() {
        return Ok(Stage1TensorParityMetric {
            element_count: 0,
            cosine_similarity: 1.0,
            max_abs: 0.0,
            mae: 0.0,
            rmse: 0.0,
            actual: actual_stats,
            reference: reference_stats,
        });
    }

    let mut max_abs = 0.0_f32;
    let mut abs_sum = 0.0_f64;
    let mut squared_sum = 0.0_f64;
    let mut dot = 0.0_f64;
    let mut actual_norm = 0.0_f64;
    let mut reference_norm = 0.0_f64;
    for (actual_value, expected_value) in actual.iter().zip(reference.iter()) {
        let diff = *actual_value - *expected_value;
        max_abs = max_abs.max(diff.abs());
        abs_sum += f64::from(diff.abs());
        squared_sum += f64::from(diff * diff);
        dot += f64::from(*actual_value) * f64::from(*expected_value);
        actual_norm += f64::from(*actual_value) * f64::from(*actual_value);
        reference_norm += f64::from(*expected_value) * f64::from(*expected_value);
    }
    let element_count = actual.len();
    let cosine_similarity = if actual_norm <= f64::EPSILON || reference_norm <= f64::EPSILON {
        1.0
    } else {
        (dot / (actual_norm.sqrt() * reference_norm.sqrt())) as f32
    };
    Ok(Stage1TensorParityMetric {
        element_count,
        cosine_similarity,
        max_abs,
        mae: (abs_sum / element_count as f64) as f32,
        rmse: (squared_sum / element_count as f64).sqrt() as f32,
        actual: actual_stats,
        reference: reference_stats,
    })
}

fn stage1_tensor_stats(values: &[f32]) -> Stage1TensorStats {
    if values.is_empty() {
        return Stage1TensorStats {
            min: 0.0,
            max: 0.0,
            mean: 0.0,
            std: 0.0,
        };
    }
    let mut min = f32::INFINITY;
    let mut max = f32::NEG_INFINITY;
    let mut sum = 0.0_f64;
    for value in values {
        min = min.min(*value);
        max = max.max(*value);
        sum += f64::from(*value);
    }
    let mean = (sum / values.len() as f64) as f32;
    let variance = values
        .iter()
        .map(|value| {
            let centered = *value - mean;
            f64::from(centered * centered)
        })
        .sum::<f64>()
        / values.len() as f64;
    Stage1TensorStats {
        min,
        max,
        mean,
        std: variance.sqrt() as f32,
    }
}

fn inspect_stage1_model_config(
    weights_path: impl AsRef<Path>,
    expected_codebooks: usize,
) -> Result<Stage1ModelConfig> {
    let weights_path = weights_path.as_ref();
    let tensors = if weights_path
        .extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| ext.eq_ignore_ascii_case("gguf"))
    {
        read_gguf_header(weights_path)?
    } else {
        read_safetensor_header(weights_path)?
    };
    let quantizer_regex = Regex::new(r"^quantizer\.quantizers\.(\d+)\.").map_err(|error| {
        OmniVoiceError::InvalidData(format!("failed to build quantizer regex: {error}"))
    })?;
    let block_regex =
        Regex::new(r"^acoustic_decoder\.block\.(\d+)\.conv_t1\.weight$").map_err(|error| {
            OmniVoiceError::InvalidData(format!("failed to build decoder regex: {error}"))
        })?;

    let mut quantizer_indices = tensors
        .keys()
        .filter_map(|name| {
            quantizer_regex
                .captures(name)
                .and_then(|captures| captures.get(1))
                .and_then(|value| value.as_str().parse::<usize>().ok())
        })
        .collect::<Vec<_>>();
    quantizer_indices.sort_unstable();
    quantizer_indices.dedup();

    if quantizer_indices.len() != expected_codebooks {
        return Err(OmniVoiceError::InvalidData(format!(
            "stage1 weights expose {} quantizers but manifest requires {}",
            quantizer_indices.len(),
            expected_codebooks
        )));
    }
    let expected_indices = (0..expected_codebooks).collect::<Vec<_>>();
    if quantizer_indices != expected_indices {
        return Err(OmniVoiceError::InvalidData(format!(
            "stage1 quantizer indices {:?} do not match expected {:?}",
            quantizer_indices, expected_indices
        )));
    }

    let first_quantizer = quantizer_indices[0];
    let codebook_shape = shape2(tensor_shape(
        &tensors,
        &format!("quantizer.quantizers.{first_quantizer}.codebook.embed"),
    )?)?;
    let project_out_shape = shape2(tensor_shape(
        &tensors,
        &format!("quantizer.quantizers.{first_quantizer}.project_out.weight"),
    )?)?;
    let fc2_shape = shape2(tensor_shape(&tensors, "fc2.weight")?)?;
    let decoder_conv1_shape = shape3(tensor_shape(&tensors, "acoustic_decoder.conv1.weight")?)?;

    if codebook_shape.1 != project_out_shape.1 {
        return Err(OmniVoiceError::InvalidData(format!(
            "stage1 codebook dim {} does not match project_out input {}",
            codebook_shape.1, project_out_shape.1
        )));
    }
    if project_out_shape.0 != fc2_shape.1 {
        return Err(OmniVoiceError::InvalidData(format!(
            "stage1 project_out dim {} does not match fc2 input {}",
            project_out_shape.0, fc2_shape.1
        )));
    }
    if fc2_shape.0 != decoder_conv1_shape.1 {
        return Err(OmniVoiceError::InvalidData(format!(
            "stage1 fc2 output {} does not match acoustic decoder input {}",
            fc2_shape.0, decoder_conv1_shape.1
        )));
    }

    let mut blocks = tensors
        .keys()
        .filter_map(|name| {
            block_regex
                .captures(name)
                .and_then(|captures| captures.get(1))
                .and_then(|value| value.as_str().parse::<usize>().ok())
        })
        .collect::<Vec<_>>();
    blocks.sort_unstable();
    blocks.dedup();

    if blocks.is_empty() {
        return Err(OmniVoiceError::InvalidData(
            "stage1 acoustic decoder blocks are missing".to_string(),
        ));
    }
    let expected_blocks = (0..blocks.len()).collect::<Vec<_>>();
    if blocks != expected_blocks {
        return Err(OmniVoiceError::InvalidData(format!(
            "stage1 acoustic decoder blocks {:?} do not match expected {:?}",
            blocks, expected_blocks
        )));
    }

    let decoder_strides = blocks
        .iter()
        .map(|index| {
            let shape = shape3(tensor_shape(
                &tensors,
                &format!("acoustic_decoder.block.{index}.conv_t1.weight"),
            )?)?;
            if shape.2 % 2 != 0 {
                return Err(OmniVoiceError::InvalidData(format!(
                    "stage1 conv_t1 kernel {} for block {index} is not even",
                    shape.2
                )));
            }
            Ok(shape.2 / 2)
        })
        .collect::<Result<Vec<_>>>()?;

    Ok(Stage1ModelConfig {
        active_quantizer_indices: quantizer_indices,
        codebook_size: codebook_shape.0,
        codebook_dim: codebook_shape.1,
        quantizer_output_dim: project_out_shape.0,
        decoder_input_dim: fc2_shape.0,
        decoder_hidden_dim: decoder_conv1_shape.0,
        decoder_strides,
    })
}

#[derive(Debug, Deserialize)]
struct SafetensorHeaderEntry {
    shape: Vec<usize>,
}

fn read_safetensor_header(
    path: impl AsRef<Path>,
) -> Result<std::collections::BTreeMap<String, SafetensorHeaderEntry>> {
    let path = path.as_ref();
    let mut file = fs::File::open(path)?;
    let file_len = file.metadata()?.len();
    if file_len < 8 {
        return Err(OmniVoiceError::InvalidData(format!(
            "safetensors file is shorter than its header prefix: {}",
            path.display()
        )));
    }
    let mut header_len_bytes = [0_u8; 8];
    file.read_exact(&mut header_len_bytes)?;
    let header_len = usize::try_from(u64::from_le_bytes(header_len_bytes)).map_err(|_| {
        OmniVoiceError::InvalidData(format!(
            "safetensors header length does not fit into usize: {}",
            path.display()
        ))
    })?;
    const MAX_SAFETENSORS_HEADER_BYTES: usize = 64 * 1024 * 1024;
    if header_len > MAX_SAFETENSORS_HEADER_BYTES {
        return Err(OmniVoiceError::InvalidData(format!(
            "safetensors header is too large ({} bytes): {}",
            header_len,
            path.display()
        )));
    }
    let header_fits = u64::try_from(header_len)
        .map(|length| length <= file_len.saturating_sub(8))
        .unwrap_or(false);
    if !header_fits {
        return Err(OmniVoiceError::InvalidData(format!(
            "safetensors header extends past end of file: {}",
            path.display()
        )));
    }
    let mut header_buffer = vec![0_u8; header_len];
    file.seek(SeekFrom::Start(8))?;
    file.read_exact(&mut header_buffer)?;
    let header = std::str::from_utf8(&header_buffer).map_err(|error| {
        OmniVoiceError::InvalidData(format!(
            "invalid safetensors header utf-8 for {}: {error}",
            path.display()
        ))
    })?;
    let parsed: std::collections::BTreeMap<String, serde_json::Value> =
        serde_json::from_str(header)?;
    parsed
        .into_iter()
        .filter(|(name, _)| name != "__metadata__")
        .map(|(name, value)| {
            serde_json::from_value::<SafetensorHeaderEntry>(value)
                .map(|entry| (name, entry))
                .map_err(|error| {
                    OmniVoiceError::InvalidData(format!(
                        "invalid tensor metadata entry in {}: {error}",
                        path.display()
                    ))
                })
        })
        .collect()
}

fn read_gguf_header(
    path: impl AsRef<Path>,
) -> Result<std::collections::BTreeMap<String, SafetensorHeaderEntry>> {
    let path = path.as_ref();
    let mut file = fs::File::open(path)?;
    let content = gguf_file::Content::read(&mut file)?;
    let mut header = std::collections::BTreeMap::new();
    for (name, info) in content.tensor_infos {
        header.insert(
            name,
            SafetensorHeaderEntry {
                shape: info.shape.dims().to_vec(),
            },
        );
    }
    Ok(header)
}

fn tensor_shape<'a>(
    header: &'a std::collections::BTreeMap<String, SafetensorHeaderEntry>,
    name: &str,
) -> Result<&'a [usize]> {
    header
        .get(name)
        .map(|entry| entry.shape.as_slice())
        .ok_or_else(|| OmniVoiceError::InvalidData(format!("missing stage1 tensor {name}")))
}

fn shape2(shape: &[usize]) -> Result<(usize, usize)> {
    if let [a, b] = shape {
        Ok((*a, *b))
    } else {
        Err(OmniVoiceError::InvalidData(format!(
            "expected 2D stage1 shape, got {shape:?}"
        )))
    }
}

fn shape3(shape: &[usize]) -> Result<(usize, usize, usize)> {
    if let [a, b, c] = shape {
        Ok((*a, *b, *c))
    } else {
        Err(OmniVoiceError::InvalidData(format!(
            "expected 3D stage1 shape, got {shape:?}"
        )))
    }
}
