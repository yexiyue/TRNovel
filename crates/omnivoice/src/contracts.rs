use std::path::Path;

use candle_core::{DType, Device, Tensor};

use crate::error::{OmniVoiceError, Result};

#[derive(Clone, Debug, PartialEq)]
pub struct I64Tensor2 {
    dims: (usize, usize),
    pub data: Vec<i64>,
}

impl I64Tensor2 {
    pub fn new(dims: (usize, usize), data: Vec<i64>) -> Result<Self> {
        let expected = dims.0 * dims.1;
        if data.len() != expected {
            return Err(OmniVoiceError::InvalidData(format!(
                "I64Tensor2 expected {} elements, got {}",
                expected,
                data.len()
            )));
        }
        Ok(Self { dims, data })
    }

    pub fn zeros(dims: (usize, usize)) -> Self {
        Self {
            dims,
            data: vec![0; dims.0 * dims.1],
        }
    }

    pub fn dims(&self) -> (usize, usize) {
        self.dims
    }

    pub fn get(&self, row: usize, col: usize) -> i64 {
        self.data[(row * self.dims.1) + col]
    }

    pub fn set(&mut self, row: usize, col: usize, value: i64) {
        let index = (row * self.dims.1) + col;
        self.data[index] = value;
    }

    pub fn to_candle(&self, device: &Device) -> Result<Tensor> {
        Ok(Tensor::from_vec(self.data.clone(), self.dims, device)?)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct I64Tensor3 {
    dims: (usize, usize, usize),
    pub data: Vec<i64>,
}

impl I64Tensor3 {
    pub fn new(dims: (usize, usize, usize), data: Vec<i64>) -> Result<Self> {
        let expected = dims.0 * dims.1 * dims.2;
        if data.len() != expected {
            return Err(OmniVoiceError::InvalidData(format!(
                "I64Tensor3 expected {} elements, got {}",
                expected,
                data.len()
            )));
        }
        Ok(Self { dims, data })
    }

    pub fn zeros(dims: (usize, usize, usize)) -> Self {
        Self {
            dims,
            data: vec![0; dims.0 * dims.1 * dims.2],
        }
    }

    pub fn full(dims: (usize, usize, usize), value: i64) -> Self {
        Self {
            dims,
            data: vec![value; dims.0 * dims.1 * dims.2],
        }
    }

    pub fn dims(&self) -> (usize, usize, usize) {
        self.dims
    }

    pub fn get(&self, b: usize, c: usize, s: usize) -> i64 {
        let (_, channels, seq_len) = self.dims;
        self.data[(b * channels * seq_len) + (c * seq_len) + s]
    }

    pub fn set(&mut self, b: usize, c: usize, s: usize, value: i64) {
        let (_, channels, seq_len) = self.dims;
        let index = (b * channels * seq_len) + (c * seq_len) + s;
        self.data[index] = value;
    }

    pub fn to_candle(&self, device: &Device) -> Result<Tensor> {
        Ok(Tensor::from_vec(self.data.clone(), self.dims, device)?)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct F32Tensor3 {
    dims: (usize, usize, usize),
    pub data: Vec<f32>,
}

impl F32Tensor3 {
    pub fn new(dims: (usize, usize, usize), data: Vec<f32>) -> Result<Self> {
        let expected = dims.0 * dims.1 * dims.2;
        if data.len() != expected {
            return Err(OmniVoiceError::InvalidData(format!(
                "F32Tensor3 expected {} elements, got {}",
                expected,
                data.len()
            )));
        }
        Ok(Self { dims, data })
    }

    pub fn dims(&self) -> (usize, usize, usize) {
        self.dims
    }

    pub fn to_candle(&self, device: &Device) -> Result<Tensor> {
        Ok(Tensor::from_vec(self.data.clone(), self.dims, device)?)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct F32Tensor4 {
    dims: (usize, usize, usize, usize),
    pub data: Vec<f32>,
}

impl F32Tensor4 {
    pub fn new(dims: (usize, usize, usize, usize), data: Vec<f32>) -> Result<Self> {
        let expected = dims.0 * dims.1 * dims.2 * dims.3;
        if data.len() != expected {
            return Err(OmniVoiceError::InvalidData(format!(
                "F32Tensor4 expected {} elements, got {}",
                expected,
                data.len()
            )));
        }
        Ok(Self { dims, data })
    }

    pub fn dims(&self) -> (usize, usize, usize, usize) {
        self.dims
    }

    pub fn to_candle(&self, device: &Device) -> Result<Tensor> {
        Ok(Tensor::from_vec(self.data.clone(), self.dims, device)?)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoolTensor2 {
    dims: (usize, usize),
    pub data: Vec<bool>,
}

impl BoolTensor2 {
    pub fn new(dims: (usize, usize), data: Vec<bool>) -> Result<Self> {
        let expected = dims.0 * dims.1;
        if data.len() != expected {
            return Err(OmniVoiceError::InvalidData(format!(
                "BoolTensor2 expected {} elements, got {}",
                expected,
                data.len()
            )));
        }
        Ok(Self { dims, data })
    }

    pub fn zeros(dims: (usize, usize)) -> Self {
        Self {
            dims,
            data: vec![false; dims.0 * dims.1],
        }
    }

    pub fn dims(&self) -> (usize, usize) {
        self.dims
    }

    pub fn get(&self, row: usize, col: usize) -> bool {
        self.data[(row * self.dims.1) + col]
    }

    pub fn set(&mut self, row: usize, col: usize, value: bool) {
        let index = (row * self.dims.1) + col;
        self.data[index] = value;
    }

    pub fn to_candle(&self, device: &Device) -> Result<Tensor> {
        let values: Vec<u8> = self.data.iter().map(|value| u8::from(*value)).collect();
        Ok(Tensor::from_vec(values, self.dims, device)?)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BoolTensor4 {
    dims: (usize, usize, usize, usize),
    pub data: Vec<bool>,
}

impl BoolTensor4 {
    pub fn new(dims: (usize, usize, usize, usize), data: Vec<bool>) -> Result<Self> {
        let expected = dims.0 * dims.1 * dims.2 * dims.3;
        if data.len() != expected {
            return Err(OmniVoiceError::InvalidData(format!(
                "BoolTensor4 expected {} elements, got {}",
                expected,
                data.len()
            )));
        }
        Ok(Self { dims, data })
    }

    pub fn zeros(dims: (usize, usize, usize, usize)) -> Self {
        Self {
            dims,
            data: vec![false; dims.0 * dims.1 * dims.2 * dims.3],
        }
    }

    pub fn dims(&self) -> (usize, usize, usize, usize) {
        self.dims
    }

    pub fn set(&mut self, b: usize, h: usize, q: usize, k: usize, value: bool) {
        let (_, heads, q_len, k_len) = self.dims;
        let index = (b * heads * q_len * k_len) + (h * q_len * k_len) + (q * k_len) + k;
        self.data[index] = value;
    }

    pub fn to_candle(&self, device: &Device) -> Result<Tensor> {
        let values: Vec<u8> = self.data.iter().map(|value| u8::from(*value)).collect();
        Ok(Tensor::from_vec(values, self.dims, device)?)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GenerationMode {
    Auto,
    Clone,
    Design,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PromptTensorBundle {
    pub input_ids: I64Tensor3,
    pub audio_mask: BoolTensor2,
}

impl PromptTensorBundle {
    pub fn input_ids_dims(&self) -> (usize, usize, usize) {
        self.input_ids.dims()
    }

    pub fn audio_mask_dims(&self) -> (usize, usize) {
        self.audio_mask.dims()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedPrompt {
    pub mode: GenerationMode,
    pub style_text: String,
    pub full_text: String,
    pub style_token_ids: Vec<u32>,
    pub text_token_ids: Vec<u32>,
    pub prompt: PromptTensorBundle,
    pub target_start_idx: usize,
    pub total_length: usize,
    pub target_length: usize,
    pub audio_mask_id: i64,
}

impl PreparedPrompt {
    pub fn zeros(
        input_ids_dims: (usize, usize, usize),
        audio_mask_dims: (usize, usize),
        target_start_idx: usize,
        total_length: usize,
    ) -> Self {
        Self {
            mode: GenerationMode::Auto,
            style_text: String::new(),
            full_text: String::new(),
            style_token_ids: Vec::new(),
            text_token_ids: Vec::new(),
            prompt: PromptTensorBundle {
                input_ids: I64Tensor3::zeros(input_ids_dims),
                audio_mask: BoolTensor2::zeros(audio_mask_dims),
            },
            target_start_idx,
            total_length,
            target_length: total_length.saturating_sub(target_start_idx),
            audio_mask_id: 1024,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ChunkedPreparedPrompts {
    pub prompts: Vec<PreparedPrompt>,
    pub chunk_texts: Vec<String>,
    pub chunk_target_lens: Vec<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PreparedPromptSequence {
    Single(PreparedPrompt),
    Chunked(ChunkedPreparedPrompts),
}

impl PreparedPromptSequence {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Single(_) => "single",
            Self::Chunked(_) => "chunked",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct BatchedInputs {
    pub batch_input_ids: I64Tensor3,
    pub batch_audio_mask: BoolTensor2,
    pub batch_attention_mask: BoolTensor4,
    pub tokens_init: I64Tensor3,
    pub schedules: Vec<Vec<usize>>,
}

impl BatchedInputs {
    pub fn zeros(
        input_ids_dims: (usize, usize, usize),
        audio_mask_dims: (usize, usize),
        attention_mask_dims: (usize, usize, usize, usize),
        tokens_init_dims: (usize, usize, usize),
    ) -> Self {
        Self {
            batch_input_ids: I64Tensor3::zeros(input_ids_dims),
            batch_audio_mask: BoolTensor2::zeros(audio_mask_dims),
            batch_attention_mask: BoolTensor4::zeros(attention_mask_dims),
            tokens_init: I64Tensor3::zeros(tokens_init_dims),
            schedules: Vec::new(),
        }
    }

    pub fn batch_input_ids_dims(&self) -> (usize, usize, usize) {
        self.batch_input_ids.dims()
    }

    pub fn batch_audio_mask_dims(&self) -> (usize, usize) {
        self.batch_audio_mask.dims()
    }

    pub fn batch_attention_mask_dims(&self) -> (usize, usize, usize, usize) {
        self.batch_attention_mask.dims()
    }

    pub fn tokens_init_dims(&self) -> (usize, usize, usize) {
        self.tokens_init.dims()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenerationRequest {
    pub texts: Vec<String>,
    pub languages: Vec<Option<String>>,
    pub ref_audios: Vec<Option<ReferenceAudioInput>>,
    pub ref_texts: Vec<Option<String>>,
    pub instructs: Vec<Option<String>>,
    pub voice_clone_prompts: Vec<Option<VoiceClonePrompt>>,
    pub speeds: Vec<Option<f32>>,
    pub durations: Vec<Option<f32>>,
    pub asr_model: Option<String>,
    pub generation_config: GenerationConfig,
}

impl GenerationRequest {
    pub fn new_text_only(text: impl Into<String>) -> Self {
        Self {
            texts: vec![text.into()],
            languages: vec![None],
            ref_audios: vec![None],
            ref_texts: vec![None],
            instructs: vec![None],
            voice_clone_prompts: vec![None],
            speeds: vec![None],
            durations: vec![None],
            asr_model: None,
            generation_config: GenerationConfig::default(),
        }
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.languages = vec![Some(language.into())];
        self
    }

    pub fn with_duration(mut self, duration_seconds: f32) -> Self {
        self.durations = vec![Some(duration_seconds)];
        self
    }

    pub fn with_ref_audio(mut self, ref_audio: ReferenceAudioInput) -> Self {
        self.ref_audios = vec![Some(ref_audio)];
        self
    }

    pub fn with_ref_text(mut self, ref_text: impl Into<String>) -> Self {
        self.ref_texts = vec![Some(ref_text.into())];
        self
    }

    pub fn with_instruct(mut self, instruct: impl Into<String>) -> Self {
        self.instructs = vec![Some(instruct.into())];
        self
    }

    pub fn with_voice_clone_prompt(mut self, prompt: VoiceClonePrompt) -> Self {
        self.voice_clone_prompts = vec![Some(prompt)];
        self
    }

    pub fn with_asr_model(mut self, asr_model: impl Into<String>) -> Self {
        self.asr_model = Some(asr_model.into());
        self
    }

    pub fn with_generation_config(mut self, generation_config: GenerationConfig) -> Self {
        self.generation_config = generation_config;
        self
    }

    pub fn denoise(&self) -> bool {
        self.generation_config.denoise
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VoiceClonePrompt {
    pub ref_audio_tokens: I64Tensor2,
    pub ref_text: String,
    pub ref_rms: Option<f32>,
}

impl VoiceClonePrompt {
    pub fn new_empty(ref_text: impl Into<String>) -> Self {
        Self {
            ref_audio_tokens: I64Tensor2::zeros((0, 0)),
            ref_text: ref_text.into(),
            ref_rms: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReferenceAudioInput {
    FilePath(String),
    Waveform(WaveformInput),
}

impl ReferenceAudioInput {
    pub fn from_path(path: impl AsRef<Path>) -> Self {
        Self::FilePath(path.as_ref().to_string_lossy().into_owned())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct WaveformInput {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: usize,
}

impl WaveformInput {
    pub fn mono(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            samples,
            sample_rate,
            channels: 1,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenerationConfig {
    pub num_step: usize,
    pub guidance_scale: f32,
    pub t_shift: f32,
    pub layer_penalty_factor: f32,
    pub position_temperature: f32,
    pub class_temperature: f32,
    pub denoise: bool,
    pub preprocess_prompt: bool,
    pub postprocess_output: bool,
    pub audio_chunk_duration: f32,
    pub audio_chunk_threshold: f32,
}

impl Default for GenerationConfig {
    fn default() -> Self {
        Self {
            num_step: 32,
            guidance_scale: 2.0,
            t_shift: 0.1,
            layer_penalty_factor: 5.0,
            position_temperature: 5.0,
            class_temperature: 0.0,
            denoise: true,
            preprocess_prompt: true,
            postprocess_output: true,
            audio_chunk_duration: 15.0,
            audio_chunk_threshold: 30.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct GenerationTask {
    pub texts: Vec<String>,
    pub target_lens: Vec<usize>,
    pub langs: Vec<Option<String>>,
    pub instructs: Vec<Option<String>>,
    pub ref_texts: Vec<Option<String>>,
    pub ref_audio_tokens: Vec<Option<I64Tensor2>>,
    pub ref_rms: Vec<Option<f32>>,
    pub speed: Vec<f32>,
    pub generation_config: GenerationConfig,
}

impl GenerationTask {
    pub fn batch_size(&self) -> usize {
        self.texts.len()
    }

    pub fn target_lens(&self) -> &[usize] {
        &self.target_lens
    }

    pub fn get_indices(&self, frame_rate: usize) -> (Vec<usize>, Vec<usize>) {
        let threshold =
            (self.generation_config.audio_chunk_threshold * frame_rate as f32).max(1.0) as usize;
        let mut short_idx = Vec::new();
        let mut long_idx = Vec::new();
        for (index, target_len) in self.target_lens.iter().copied().enumerate() {
            if target_len <= threshold {
                short_idx.push(index);
            } else {
                long_idx.push(index);
            }
        }
        (short_idx, long_idx)
    }

    pub fn slice_task(&self, indices: &[usize]) -> Self {
        Self {
            texts: indices
                .iter()
                .map(|&index| self.texts[index].clone())
                .collect(),
            target_lens: indices
                .iter()
                .map(|&index| self.target_lens[index])
                .collect(),
            langs: indices
                .iter()
                .map(|&index| self.langs[index].clone())
                .collect(),
            instructs: indices
                .iter()
                .map(|&index| self.instructs[index].clone())
                .collect(),
            ref_texts: indices
                .iter()
                .map(|&index| self.ref_texts[index].clone())
                .collect(),
            ref_audio_tokens: indices
                .iter()
                .map(|&index| self.ref_audio_tokens[index].clone())
                .collect(),
            ref_rms: indices.iter().map(|&index| self.ref_rms[index]).collect(),
            speed: indices.iter().map(|&index| self.speed[index]).collect(),
            generation_config: self.generation_config.clone(),
        }
    }

    pub fn mode_for(&self, index: usize) -> GenerationMode {
        if self
            .ref_audio_tokens
            .get(index)
            .is_some_and(Option::is_some)
        {
            GenerationMode::Clone
        } else if self.instructs.get(index).is_some_and(|item| item.is_some()) {
            GenerationMode::Design
        } else {
            GenerationMode::Auto
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stage0Output {
    pub inputs_embeds_dims: (usize, usize, usize),
    pub hidden_states_dims: (usize, usize, usize),
    pub logits_dims: (usize, usize, usize, usize),
}

#[derive(Clone, Debug, PartialEq)]
pub enum GeneratedTokens {
    Single(I64Tensor2),
    Chunked(Vec<I64Tensor2>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GenerationUsage {
    pub input_tokens: usize,
    pub output_tokens: usize,
    pub total_tokens: usize,
}

impl GenerationUsage {
    pub fn new(input_tokens: usize, output_tokens: usize) -> Self {
        Self {
            input_tokens,
            output_tokens,
            total_tokens: input_tokens + output_tokens,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DecodedAudio {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct GeneratedAudioResult {
    pub audio: DecodedAudio,
    pub usage: GenerationUsage,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioParityMetrics {
    pub sample_rate: u32,
    pub frame_count: usize,
    pub reference_frame_count: usize,
    pub max_abs: f32,
    pub mae: f32,
    pub rmse: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TensorParityMetrics {
    pub element_count: usize,
    pub max_abs: f32,
    pub mae: f32,
    pub rmse: f32,
}

impl DecodedAudio {
    pub fn new(samples: Vec<f32>, sample_rate: u32) -> Self {
        Self {
            samples,
            sample_rate,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.sample_rate == 0 {
            return Err(OmniVoiceError::InvalidData(
                "decoded audio sample rate must be greater than zero".to_string(),
            ));
        }
        if let Some(index) = self.samples.iter().position(|sample| !sample.is_finite()) {
            return Err(OmniVoiceError::InvalidData(format!(
                "decoded audio sample at index {index} is not finite"
            )));
        }
        Ok(())
    }

    pub fn frame_count(&self) -> usize {
        self.samples.len()
    }

    pub fn write_wav(&self, path: impl AsRef<Path>) -> Result<()> {
        self.validate()?;
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: self.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec)?;
        for sample in &self.samples {
            writer.write_sample(*sample)?;
        }
        writer.finalize()?;
        Ok(())
    }

    pub fn read_wav(path: impl AsRef<Path>) -> Result<Self> {
        let mut reader = hound::WavReader::open(path)?;
        let spec = reader.spec();
        let samples = match spec.sample_format {
            hound::SampleFormat::Float => reader
                .samples::<f32>()
                .collect::<std::result::Result<Vec<_>, _>>()?,
            hound::SampleFormat::Int => match spec.bits_per_sample {
                8 => reader
                    .samples::<i8>()
                    .map(|sample| sample.map(|value| value as f32 / 128.0))
                    .collect::<std::result::Result<Vec<_>, _>>()?,
                16 => reader
                    .samples::<i16>()
                    .map(|sample| sample.map(|value| value as f32 / 32768.0))
                    .collect::<std::result::Result<Vec<_>, _>>()?,
                24 => reader
                    .samples::<i32>()
                    .map(|sample| sample.map(|value| value as f32 / 8_388_608.0))
                    .collect::<std::result::Result<Vec<_>, _>>()?,
                32 => reader
                    .samples::<i32>()
                    .map(|sample| sample.map(|value| value as f32 / 2_147_483_648.0))
                    .collect::<std::result::Result<Vec<_>, _>>()?,
                bits => {
                    return Err(OmniVoiceError::Unsupported(format!(
                        "unsupported WAV integer sample width {bits}"
                    )));
                }
            },
        };
        if spec.channels == 0 || !samples.len().is_multiple_of(spec.channels as usize) {
            return Err(OmniVoiceError::InvalidData(
                "WAV channel layout is invalid".to_string(),
            ));
        }
        let samples = if spec.channels == 1 {
            samples
        } else {
            let channels = spec.channels as usize;
            let mut mono = vec![0.0_f32; samples.len() / channels];
            for (index, sample) in samples.iter().enumerate() {
                mono[index / channels] += *sample;
            }
            for sample in &mut mono {
                *sample /= channels as f32;
            }
            mono
        };
        let audio = Self::new(samples, spec.sample_rate);
        audio.validate()?;
        Ok(audio)
    }

    pub fn parity_metrics(&self, reference: &Self) -> Result<AudioParityMetrics> {
        self.validate()?;
        reference.validate()?;
        if self.sample_rate != reference.sample_rate {
            return Err(OmniVoiceError::InvalidData(format!(
                "sample rate mismatch: actual={}, reference={}",
                self.sample_rate, reference.sample_rate
            )));
        }
        if self.samples.len() != reference.samples.len() {
            return Err(OmniVoiceError::InvalidData(format!(
                "frame count mismatch: actual={}, reference={}",
                self.samples.len(),
                reference.samples.len()
            )));
        }
        if self.samples.is_empty() {
            return Ok(AudioParityMetrics {
                sample_rate: self.sample_rate,
                frame_count: 0,
                reference_frame_count: 0,
                max_abs: 0.0,
                mae: 0.0,
                rmse: 0.0,
            });
        }

        let mut max_abs = 0.0_f32;
        let mut abs_sum = 0.0_f64;
        let mut squared_sum = 0.0_f64;
        for (actual, expected) in self.samples.iter().zip(reference.samples.iter()) {
            let diff = *actual - *expected;
            max_abs = max_abs.max(diff.abs());
            abs_sum += f64::from(diff.abs());
            squared_sum += f64::from(diff * diff);
        }
        let frame_count = self.samples.len();
        Ok(AudioParityMetrics {
            sample_rate: self.sample_rate,
            frame_count,
            reference_frame_count: reference.samples.len(),
            max_abs,
            mae: (abs_sum / frame_count as f64) as f32,
            rmse: (squared_sum / frame_count as f64).sqrt() as f32,
        })
    }
}

#[derive(Debug)]
pub struct PreparedInferenceBatch {
    pub input_ids: Tensor,
    pub audio_mask: Tensor,
    pub attention_mask: Tensor,
    pub tokens_init: Tensor,
    pub target_lens: Vec<usize>,
    pub cond_lens: Vec<usize>,
    pub runtime_dtype: DType,
}

impl PreparedInferenceBatch {
    pub fn input_ids_dims(&self) -> Result<(usize, usize, usize)> {
        let dims = self.input_ids.dims();
        if let [a, b, c] = dims {
            Ok((*a, *b, *c))
        } else {
            Err(OmniVoiceError::InvalidTensorShape {
                name: "input_ids".to_string(),
                expected: "(B, C, S)".to_string(),
                actual: format!("{dims:?}"),
            })
        }
    }

    pub fn audio_mask_dims(&self) -> Result<(usize, usize)> {
        let dims = self.audio_mask.dims();
        if let [a, b] = dims {
            Ok((*a, *b))
        } else {
            Err(OmniVoiceError::InvalidTensorShape {
                name: "audio_mask".to_string(),
                expected: "(B, S)".to_string(),
                actual: format!("{dims:?}"),
            })
        }
    }

    pub fn attention_mask_dims(&self) -> Result<(usize, usize, usize, usize)> {
        let dims = self.attention_mask.dims();
        if let [a, b, c, d] = dims {
            Ok((*a, *b, *c, *d))
        } else {
            Err(OmniVoiceError::InvalidTensorShape {
                name: "attention_mask".to_string(),
                expected: "(B, H, Q, K)".to_string(),
                actual: format!("{dims:?}"),
            })
        }
    }

    pub fn tokens_init_dims(&self) -> Result<(usize, usize, usize)> {
        let dims = self.tokens_init.dims();
        if let [a, b, c] = dims {
            Ok((*a, *b, *c))
        } else {
            Err(OmniVoiceError::InvalidTensorShape {
                name: "tokens_init".to_string(),
                expected: "(B, C, T)".to_string(),
                actual: format!("{dims:?}"),
            })
        }
    }
}
