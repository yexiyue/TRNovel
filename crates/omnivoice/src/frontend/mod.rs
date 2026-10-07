use std::{fs, path::Path};

use candle_core::quantized::gguf_file;
use candle_core::{Device, Tensor};
use serde::Deserialize;
use tokenizers::{AddedToken, Tokenizer};

use crate::{
    BoolTensor2,
    artifacts::{RuntimeArtifacts, gguf_u32},
    contracts::{
        GenerationConfig, GenerationRequest, GenerationTask, I64Tensor3, PreparedPrompt,
        PromptTensorBundle, VoiceClonePrompt,
    },
    error::{OmniVoiceError, Result},
};

mod duration;
mod language;
mod text;
mod voice_design;

pub use duration::RuleDurationEstimator;
pub use language::resolve_language;
pub use text::{
    add_punctuation, chunk_text_punctuation, combine_text, tokenize_with_nonverbal_tags,
};
pub use voice_design::{contains_cjk, resolve_instruct};

#[derive(Debug, Clone)]
pub struct Frontend {
    tokenizer: Tokenizer,
    num_audio_codebook: usize,
    audio_mask_id: i64,
    frame_rate: usize,
    duration_estimator: RuleDurationEstimator,
}

const MAX_TARGET_TOKENS: usize = 1_000_000;
const MAX_GENERATION_STEPS: usize = 4096;

#[derive(Debug, Clone)]
pub(crate) struct DeviceVoiceClonePrompt {
    pub ref_audio_tokens: Tensor,
    pub ref_text: String,
    pub ref_rms: Option<f32>,
}

#[derive(Debug, Clone)]
pub(crate) struct DeviceGenerationTask {
    pub texts: Vec<String>,
    pub target_lens: Vec<usize>,
    pub langs: Vec<Option<String>>,
    pub instructs: Vec<Option<String>>,
    pub ref_texts: Vec<Option<String>>,
    pub ref_audio_tokens: Vec<Option<Tensor>>,
    pub ref_rms: Vec<Option<f32>>,
    pub speed: Vec<f32>,
    pub generation_config: crate::contracts::GenerationConfig,
}

impl DeviceGenerationTask {
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
}

#[derive(Debug)]
pub(crate) struct PreparedPromptDevice {
    pub input_ids: Tensor,
    pub audio_mask: Tensor,
    pub total_length: usize,
    pub audio_mask_id: i64,
}

#[derive(Debug, Deserialize)]
struct ModelConfigFile {
    num_audio_codebook: usize,
    audio_mask_id: i64,
}

const OMNIVOICE_GGUF_SPECIAL_KEYS: [&str; 7] = [
    "omnivoice.special.denoise",
    "omnivoice.special.lang_start",
    "omnivoice.special.lang_end",
    "omnivoice.special.instruct_start",
    "omnivoice.special.instruct_end",
    "omnivoice.special.text_start",
    "omnivoice.special.text_end",
];

fn register_omnivoice_gguf_specials(
    tokenizer: &mut Tokenizer,
    content: &gguf_file::Content,
) -> Result<()> {
    let mut specials = Vec::with_capacity(OMNIVOICE_GGUF_SPECIAL_KEYS.len());
    let mut expected_ids = Vec::with_capacity(OMNIVOICE_GGUF_SPECIAL_KEYS.len());
    for key in OMNIVOICE_GGUF_SPECIAL_KEYS {
        let id = gguf_u32(content, key)?;
        let token = tokenizer.id_to_token(id).ok_or_else(|| {
            OmniVoiceError::InvalidData(format!(
                "GGUF special `{key}` id {id} is missing from the tokenizer vocab"
            ))
        })?;
        if token.is_empty() {
            return Err(OmniVoiceError::InvalidData(format!(
                "GGUF special `{key}` id {id} has an empty vocab string"
            )));
        }
        specials.push(
            AddedToken::from(&token, true)
                .normalized(false)
                .lstrip(false)
                .rstrip(false)
                .single_word(false),
        );
        expected_ids.push((key, token, id));
    }
    // tokenizers 0.23 takes owned AddedToken items (0.22 accepted references).
    tokenizer.add_special_tokens(&specials);
    for (key, token, expected_id) in expected_ids {
        let actual_id = tokenizer.token_to_id(&token).ok_or_else(|| {
            OmniVoiceError::InvalidData(format!(
                "GGUF special `{key}` token `{token}` was not registered"
            ))
        })?;
        if actual_id != expected_id {
            return Err(OmniVoiceError::InvalidData(format!(
                "GGUF special `{key}` id shifted from {expected_id} to {actual_id}"
            )));
        }
    }
    Ok(())
}

impl Frontend {
    pub fn from_model_root(model_root: impl AsRef<Path>) -> Result<Self> {
        let runtime = RuntimeArtifacts::from_model_root(model_root)?;
        Self::from_runtime_artifacts(&runtime)
    }

    pub fn from_runtime_artifacts(runtime: &RuntimeArtifacts) -> Result<Self> {
        let (tokenizer, model_config) = if let Some(gguf) = runtime.gguf_bundle() {
            let content = gguf.open_generator_content()?;
            let mut tokenizer = Tokenizer::from_file(runtime.model_root().join("tokenizer.json"))?;
            register_omnivoice_gguf_specials(&mut tokenizer, &content)?;
            let model_config = ModelConfigFile {
                num_audio_codebook: runtime.contracts().num_audio_codebooks,
                audio_mask_id: runtime.contracts().audio_mask_id,
            };
            (tokenizer, model_config)
        } else {
            let tokenizer = Tokenizer::from_file(runtime.text_tokenizer()?.tokenizer_path())?;
            let model_config: ModelConfigFile =
                serde_json::from_str(&fs::read_to_string(runtime.generator()?.config_path())?)?;
            (tokenizer, model_config)
        };
        Ok(Self {
            tokenizer,
            num_audio_codebook: model_config.num_audio_codebook,
            audio_mask_id: model_config.audio_mask_id,
            frame_rate: runtime.contracts().frame_rate,
            duration_estimator: RuleDurationEstimator,
        })
    }

    pub fn build_task(&self, request: &GenerationRequest) -> Result<GenerationTask> {
        validate_generation_config(&request.generation_config)?;
        let batch_size = request.texts.len();
        if batch_size == 0 {
            return Err(OmniVoiceError::InvalidRequest(
                "generation request must contain at least one text".to_string(),
            ));
        }
        let languages = normalize_option_list(&request.languages, batch_size, "languages")?
            .into_iter()
            .map(|item| resolve_language(item.as_deref()))
            .collect();
        normalize_option_list(&request.ref_texts, batch_size, "ref_texts")?;
        let instructs_raw = normalize_option_list(&request.instructs, batch_size, "instructs")?;
        let voice_clone_prompts =
            normalize_option_voice_prompts(&request.voice_clone_prompts, batch_size)?;
        let requested_speeds = normalize_option_f32_list(&request.speeds, batch_size)?;
        let durations = normalize_option_f32_list(&request.durations, batch_size)?;
        validate_generation_scalars(&requested_speeds, &durations, self.frame_rate)?;

        let mut instructs = Vec::with_capacity(batch_size);
        for (index, item) in instructs_raw.into_iter().enumerate() {
            let use_zh = contains_cjk(&request.texts[index]);
            instructs.push(resolve_instruct(item.as_deref(), use_zh)?);
        }

        let mut ref_texts = Vec::with_capacity(batch_size);
        let mut ref_audio_tokens = Vec::with_capacity(batch_size);
        let mut ref_rms = Vec::with_capacity(batch_size);
        let mut target_lens = Vec::with_capacity(batch_size);
        let mut effective_speeds = Vec::with_capacity(batch_size);

        for (index, text) in request.texts.iter().enumerate() {
            let (reference_text, reference_audio_tokens, reference_rms) =
                if let Some(prompt) = voice_clone_prompts[index].clone() {
                    (
                        Some(prompt.ref_text),
                        Some(prompt.ref_audio_tokens),
                        prompt.ref_rms,
                    )
                } else {
                    (None, None, None)
                };

            let requested_speed = requested_speeds[index].unwrap_or(1.0);
            let estimated_target_length = self.estimate_target_tokens(
                text,
                reference_text.as_deref(),
                reference_audio_tokens
                    .as_ref()
                    .map(|tokens| tokens.dims().1),
                if durations[index].is_some() {
                    1.0
                } else {
                    requested_speed
                },
            );

            let (target_length, effective_speed) = if let Some(duration_seconds) = durations[index]
            {
                let target_tokens = checked_duration_tokens(duration_seconds, self.frame_rate)?;
                let speed = if target_tokens > 0 {
                    estimated_target_length as f32 / target_tokens as f32
                } else {
                    1.0
                };
                (target_tokens, speed)
            } else {
                (estimated_target_length, requested_speed)
            };

            ref_texts.push(reference_text);
            ref_audio_tokens.push(reference_audio_tokens);
            ref_rms.push(reference_rms);
            target_lens.push(target_length);
            effective_speeds.push(effective_speed);
        }

        Ok(GenerationTask {
            texts: request.texts.clone(),
            target_lens,
            langs: languages,
            instructs,
            ref_texts,
            ref_audio_tokens,
            ref_rms,
            speed: effective_speeds,
            generation_config: request.generation_config.clone(),
        })
    }

    pub(crate) fn build_task_with_device_prompts(
        &self,
        request: &GenerationRequest,
        voice_clone_prompts: &[Option<DeviceVoiceClonePrompt>],
    ) -> Result<DeviceGenerationTask> {
        validate_generation_config(&request.generation_config)?;
        let batch_size = request.texts.len();
        if batch_size == 0 {
            return Err(OmniVoiceError::InvalidRequest(
                "generation request must contain at least one text".to_string(),
            ));
        }
        if voice_clone_prompts.len() != batch_size {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "device voice_clone_prompts should contain {batch_size} items, got {}",
                voice_clone_prompts.len()
            )));
        }
        let languages = normalize_option_list(&request.languages, batch_size, "languages")?
            .into_iter()
            .map(|item| resolve_language(item.as_deref()))
            .collect();
        normalize_option_list(&request.ref_texts, batch_size, "ref_texts")?;
        let instructs_raw = normalize_option_list(&request.instructs, batch_size, "instructs")?;
        let requested_speeds = normalize_option_f32_list(&request.speeds, batch_size)?;
        let durations = normalize_option_f32_list(&request.durations, batch_size)?;
        validate_generation_scalars(&requested_speeds, &durations, self.frame_rate)?;

        let mut instructs = Vec::with_capacity(batch_size);
        for (index, item) in instructs_raw.into_iter().enumerate() {
            let use_zh = contains_cjk(&request.texts[index]);
            instructs.push(resolve_instruct(item.as_deref(), use_zh)?);
        }

        let mut ref_texts = Vec::with_capacity(batch_size);
        let mut ref_audio_tokens = Vec::with_capacity(batch_size);
        let mut ref_rms = Vec::with_capacity(batch_size);
        let mut target_lens = Vec::with_capacity(batch_size);
        let mut effective_speeds = Vec::with_capacity(batch_size);

        for (index, text) in request.texts.iter().enumerate() {
            let (reference_text, reference_audio_tokens, reference_rms) =
                if let Some(prompt) = voice_clone_prompts[index].clone() {
                    (
                        Some(prompt.ref_text),
                        Some(prompt.ref_audio_tokens),
                        prompt.ref_rms,
                    )
                } else {
                    (None, None, None)
                };

            let requested_speed = requested_speeds[index].unwrap_or(1.0);
            let estimated_target_length = self.estimate_target_tokens(
                text,
                reference_text.as_deref(),
                reference_audio_tokens
                    .as_ref()
                    .map(|tokens| tokens.dims2())
                    .transpose()?
                    .map(|(_, steps)| steps),
                if durations[index].is_some() {
                    1.0
                } else {
                    requested_speed
                },
            );

            let (target_length, effective_speed) = if let Some(duration_seconds) = durations[index]
            {
                let target_tokens = checked_duration_tokens(duration_seconds, self.frame_rate)?;
                let speed = if target_tokens > 0 {
                    estimated_target_length as f32 / target_tokens as f32
                } else {
                    1.0
                };
                (target_tokens, speed)
            } else {
                (estimated_target_length, requested_speed)
            };

            ref_texts.push(reference_text);
            ref_audio_tokens.push(reference_audio_tokens);
            ref_rms.push(reference_rms);
            target_lens.push(target_length);
            effective_speeds.push(effective_speed);
        }

        Ok(DeviceGenerationTask {
            texts: request.texts.clone(),
            target_lens,
            langs: languages,
            instructs,
            ref_texts,
            ref_audio_tokens,
            ref_rms,
            speed: effective_speeds,
            generation_config: request.generation_config.clone(),
        })
    }

    pub fn prepare_prompt(&self, task: &GenerationTask, index: usize) -> Result<PreparedPrompt> {
        let text = task.texts.get(index).ok_or_else(|| {
            OmniVoiceError::InvalidRequest(format!("missing task item at {index}"))
        })?;
        let lang = task.langs.get(index).cloned().flatten();
        let instruct = task.instructs.get(index).cloned().flatten();
        let ref_text = task.ref_texts.get(index).cloned().flatten();
        let ref_audio_tokens = task.ref_audio_tokens.get(index).cloned().flatten();
        let target_length = *task.target_lens.get(index).ok_or_else(|| {
            OmniVoiceError::InvalidRequest(format!("missing target length at {index}"))
        })?;
        if target_length == 0 {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "target length at {index} must be greater than zero"
            )));
        }
        let mode = task.mode_for(index);

        let mut style_text = String::new();
        if task.generation_config.denoise && ref_audio_tokens.is_some() {
            style_text.push_str("<|denoise|>");
        }
        style_text.push_str(&format!(
            "<|lang_start|>{}<|lang_end|><|instruct_start|>{}<|instruct_end|>",
            lang.clone().unwrap_or_else(|| "None".to_string()),
            instruct.clone().unwrap_or_else(|| "None".to_string())
        ));
        let style_encoding = self.tokenizer.encode(style_text.clone(), false)?;
        let style_token_ids = style_encoding.get_ids().to_vec();

        let full_text = combine_text(text, ref_text.as_deref());
        let text_prompt = format!("<|text_start|>{full_text}<|text_end|>");
        let text_token_ids = tokenize_with_nonverbal_tags(&self.tokenizer, &text_prompt)?;

        let style_len = style_token_ids.len();
        let text_len = text_token_ids.len();
        let ref_audio_length = ref_audio_tokens
            .as_ref()
            .map(|tokens| tokens.dims().1)
            .unwrap_or(0);
        if let Some(reference_tokens) = &ref_audio_tokens {
            let (rows, _) = reference_tokens.dims();
            if rows != self.num_audio_codebook {
                return Err(OmniVoiceError::InvalidRequest(format!(
                    "reference audio tokens at index {index} expected {} codebooks, got {rows}",
                    self.num_audio_codebook
                )));
            }
        }
        let target_start_idx = style_len
            .checked_add(text_len)
            .and_then(|length| length.checked_add(ref_audio_length))
            .ok_or_else(|| OmniVoiceError::InvalidRequest("prompt length overflow".to_string()))?;
        let total_length = target_start_idx
            .checked_add(target_length)
            .ok_or_else(|| OmniVoiceError::InvalidRequest("prompt length overflow".to_string()))?;

        let mut input_ids = I64Tensor3::full(
            (1, self.num_audio_codebook, total_length),
            self.audio_mask_id,
        );
        for codebook in 0..self.num_audio_codebook {
            for (position, token_id) in style_token_ids.iter().enumerate() {
                input_ids.set(0, codebook, position, i64::from(*token_id));
            }
            for (position, token_id) in text_token_ids.iter().enumerate() {
                input_ids.set(0, codebook, style_len + position, i64::from(*token_id));
            }
            if let Some(reference_tokens) = &ref_audio_tokens {
                for position in 0..ref_audio_length {
                    input_ids.set(
                        0,
                        codebook,
                        style_len + text_len + position,
                        reference_tokens.get(codebook, position),
                    );
                }
            }
        }

        // Audio positions cover reference audio (when present) and the target
        // region. Marking only the target left clone prompts with ref tokens
        // embedded as text, which diverged subsequent long-form chunks.
        let mut audio_mask = BoolTensor2::zeros((1, total_length));
        let audio_start_idx = style_len + text_len;
        for position in audio_start_idx..total_length {
            audio_mask.set(0, position, true);
        }

        Ok(PreparedPrompt {
            mode,
            style_text,
            full_text,
            style_token_ids,
            text_token_ids,
            prompt: PromptTensorBundle {
                input_ids,
                audio_mask,
            },
            target_start_idx,
            total_length,
            target_length,
            audio_mask_id: self.audio_mask_id,
        })
    }

    pub(crate) fn prepare_prompt_device(
        &self,
        task: &DeviceGenerationTask,
        index: usize,
        device: &Device,
    ) -> Result<PreparedPromptDevice> {
        let text = task.texts.get(index).ok_or_else(|| {
            OmniVoiceError::InvalidRequest(format!("missing task item at {index}"))
        })?;
        let lang = task.langs.get(index).cloned().flatten();
        let instruct = task.instructs.get(index).cloned().flatten();
        let ref_text = task.ref_texts.get(index).cloned().flatten();
        let ref_audio_tokens = task.ref_audio_tokens.get(index).cloned().flatten();
        let target_length = *task.target_lens.get(index).ok_or_else(|| {
            OmniVoiceError::InvalidRequest(format!("missing target length at {index}"))
        })?;
        if target_length == 0 {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "target length at {index} must be greater than zero"
            )));
        }

        let mut style_text = String::new();
        if task.generation_config.denoise && ref_audio_tokens.is_some() {
            style_text.push_str("<|denoise|>");
        }
        style_text.push_str(&format!(
            "<|lang_start|>{}<|lang_end|><|instruct_start|>{}<|instruct_end|>",
            lang.clone().unwrap_or_else(|| "None".to_string()),
            instruct.clone().unwrap_or_else(|| "None".to_string())
        ));
        let style_encoding = self.tokenizer.encode(style_text, false)?;
        let style_token_ids = style_encoding.get_ids().to_vec();

        let full_text = combine_text(text, ref_text.as_deref());
        let text_prompt = format!("<|text_start|>{full_text}<|text_end|>");
        let text_token_ids = tokenize_with_nonverbal_tags(&self.tokenizer, &text_prompt)?;

        let style_len = style_token_ids.len();
        let text_len = text_token_ids.len();
        let ref_audio_length = ref_audio_tokens
            .as_ref()
            .map(|tokens| tokens.dims2())
            .transpose()?
            .map(|(rows, steps)| {
                if rows != self.num_audio_codebook {
                    return Err(OmniVoiceError::InvalidRequest(format!(
                        "reference audio tokens at index {index} expected {} codebooks, got {rows}",
                        self.num_audio_codebook
                    )));
                }
                Ok(steps)
            })
            .transpose()?
            .unwrap_or(0);
        let target_start_idx = style_len
            .checked_add(text_len)
            .and_then(|length| length.checked_add(ref_audio_length))
            .ok_or_else(|| OmniVoiceError::InvalidRequest("prompt length overflow".to_string()))?;
        let total_length = target_start_idx
            .checked_add(target_length)
            .ok_or_else(|| OmniVoiceError::InvalidRequest("prompt length overflow".to_string()))?;

        let mut input_ids_data = vec![self.audio_mask_id; self.num_audio_codebook * total_length];
        for codebook in 0..self.num_audio_codebook {
            let row_offset = codebook * total_length;
            for (position, token_id) in style_token_ids.iter().enumerate() {
                input_ids_data[row_offset + position] = i64::from(*token_id);
            }
            for (position, token_id) in text_token_ids.iter().enumerate() {
                input_ids_data[row_offset + style_len + position] = i64::from(*token_id);
            }
        }
        let input_ids = Tensor::from_vec(
            input_ids_data,
            (1, self.num_audio_codebook, total_length),
            device,
        )?;
        if let Some(reference_tokens) = ref_audio_tokens {
            input_ids.slice_set(
                &reference_tokens.unsqueeze(0)?,
                2,
                target_start_idx - ref_audio_length,
            )?;
        }

        // Include ref-audio steps in the audio mask (style/text stay false).
        let mut audio_mask_data = vec![0u8; total_length];
        let audio_start_idx = style_len + text_len;
        for mask_value in &mut audio_mask_data[audio_start_idx..] {
            *mask_value = 1;
        }
        let audio_mask = Tensor::from_vec(audio_mask_data, (1, total_length), device)?;

        Ok(PreparedPromptDevice {
            input_ids,
            audio_mask,
            total_length,
            audio_mask_id: self.audio_mask_id,
        })
    }

    pub fn frame_rate(&self) -> usize {
        self.frame_rate
    }

    pub fn chunk_text(
        &self,
        text: &str,
        target_len: usize,
        audio_chunk_duration: f32,
    ) -> Vec<String> {
        let avg_tokens_per_char = target_len as f32 / text.chars().count().max(1) as f32;
        let text_chunk_len = ((audio_chunk_duration * self.frame_rate as f32) / avg_tokens_per_char)
            .max(1.0) as usize;
        chunk_text_punctuation(text, text_chunk_len, Some(3))
    }

    pub fn estimate_target_tokens(
        &self,
        text: &str,
        ref_text: Option<&str>,
        num_ref_audio_tokens: Option<usize>,
        speed: f32,
    ) -> usize {
        let reference_duration = num_ref_audio_tokens
            .map(|tokens| tokens as f32)
            .unwrap_or(25.0);
        let estimated = self.duration_estimator.estimate_duration(
            text,
            ref_text.unwrap_or("Nice to meet you."),
            reference_duration,
            Some(50.0),
            3.0,
        );
        let adjusted = if (speed - 1.0).abs() > f32::EPSILON {
            estimated / speed
        } else {
            estimated
        };
        if !adjusted.is_finite() {
            return MAX_TARGET_TOKENS;
        }
        adjusted.clamp(1.0, MAX_TARGET_TOKENS as f32) as usize
    }

    pub fn count_text_tokens(&self, text: &str) -> Result<usize> {
        Ok(self.tokenizer.encode(text, true)?.len())
    }
}

fn normalize_option_list(
    values: &[Option<String>],
    batch_size: usize,
    name: &str,
) -> Result<Vec<Option<String>>> {
    if values.len() == batch_size {
        Ok(values.to_vec())
    } else if values.len() == 1 {
        Ok(vec![values[0].clone(); batch_size])
    } else {
        Err(OmniVoiceError::InvalidRequest(format!(
            "{name} should contain either 1 or {batch_size} items, got {}",
            values.len()
        )))
    }
}

fn normalize_option_f32_list(
    values: &[Option<f32>],
    batch_size: usize,
) -> Result<Vec<Option<f32>>> {
    if values.len() == batch_size {
        Ok(values.to_vec())
    } else if values.len() == 1 {
        Ok(vec![values[0]; batch_size])
    } else {
        Err(OmniVoiceError::InvalidRequest(format!(
            "duration list should contain either 1 or {batch_size} items, got {}",
            values.len()
        )))
    }
}

fn validate_generation_scalars(
    speeds: &[Option<f32>],
    durations: &[Option<f32>],
    frame_rate: usize,
) -> Result<()> {
    for (index, value) in speeds
        .iter()
        .enumerate()
        .filter_map(|(index, value)| value.as_ref().map(|value| (index, value)))
    {
        if !value.is_finite() || *value <= 0.0 {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "speed at index {index} must be finite and greater than zero"
            )));
        }
    }
    for (index, value) in durations
        .iter()
        .enumerate()
        .filter_map(|(index, value)| value.as_ref().map(|value| (index, value)))
    {
        if !value.is_finite() || *value <= 0.0 {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "duration at index {index} must be finite and greater than zero"
            )));
        }
        let _ = checked_duration_tokens(*value, frame_rate).map_err(|_| {
            OmniVoiceError::InvalidRequest(format!(
                "duration at index {index} is too large for the runtime"
            ))
        })?;
    }
    Ok(())
}

fn validate_generation_config(config: &GenerationConfig) -> Result<()> {
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
    if !config.audio_chunk_duration.is_finite() || config.audio_chunk_duration <= 0.0 {
        return Err(OmniVoiceError::InvalidRequest(
            "audio_chunk_duration must be finite and greater than zero".to_string(),
        ));
    }
    if !config.audio_chunk_threshold.is_finite() || config.audio_chunk_threshold <= 0.0 {
        return Err(OmniVoiceError::InvalidRequest(
            "audio_chunk_threshold must be finite and greater than zero".to_string(),
        ));
    }
    Ok(())
}

fn checked_duration_tokens(duration_seconds: f32, frame_rate: usize) -> Result<usize> {
    let tokens = duration_seconds * frame_rate as f32;
    if !tokens.is_finite() || tokens > MAX_TARGET_TOKENS as f32 {
        return Err(OmniVoiceError::InvalidRequest(format!(
            "duration must produce at most {MAX_TARGET_TOKENS} audio tokens"
        )));
    }
    Ok(tokens.max(1.0) as usize)
}

fn normalize_option_voice_prompts(
    values: &[Option<VoiceClonePrompt>],
    batch_size: usize,
) -> Result<Vec<Option<VoiceClonePrompt>>> {
    if values.len() == batch_size {
        Ok(values.to_vec())
    } else if values.len() == 1 {
        Ok(vec![values[0].clone(); batch_size])
    } else {
        Err(OmniVoiceError::InvalidRequest(format!(
            "voice_clone_prompts should contain either 1 or {batch_size} items, got {}",
            values.len()
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::{checked_duration_tokens, validate_generation_config, validate_generation_scalars};
    use crate::contracts::GenerationConfig;

    #[test]
    fn generation_scalars_reject_non_finite_and_unbounded_values() {
        assert!(validate_generation_scalars(&[Some(0.0)], &[], 25).is_err());
        assert!(validate_generation_scalars(&[Some(f32::INFINITY)], &[], 25).is_err());
        assert!(validate_generation_scalars(&[], &[Some(-1.0)], 25).is_err());
        assert!(validate_generation_scalars(&[], &[Some(f32::NAN)], 25).is_err());
        assert!(validate_generation_scalars(&[], &[Some(50_000.0)], 25).is_err());
    }

    #[test]
    fn duration_tokens_are_clamped_to_at_least_one_frame() {
        assert_eq!(checked_duration_tokens(f32::MIN_POSITIVE, 25).unwrap(), 1);
    }

    #[test]
    fn generation_config_rejects_non_finite_sampling_controls() {
        let config = GenerationConfig {
            class_temperature: f32::NAN,
            ..GenerationConfig::default()
        };
        assert!(validate_generation_config(&config).is_err());
        let config = GenerationConfig {
            audio_chunk_duration: 0.0,
            ..GenerationConfig::default()
        };
        assert!(validate_generation_config(&config).is_err());
    }
}
