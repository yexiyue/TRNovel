use crate::{
    contracts::{BatchedInputs, BoolTensor2, BoolTensor4, I64Tensor3, PreparedPrompt},
    error::{OmniVoiceError, Result},
};

#[derive(Debug, Clone, PartialEq)]
pub struct PredictionResult {
    pub pred_tokens: Vec<i64>,
    pub confidence_scores: Vec<f32>,
    pub dims: (usize, usize),
}

pub fn build_timesteps(
    t_start: f32,
    t_end: f32,
    num_step: usize,
    t_shift: f32,
) -> Result<Vec<f32>> {
    if num_step == 0 {
        return Err(OmniVoiceError::InvalidRequest(
            "num_step must be > 0".to_string(),
        ));
    }
    if !t_start.is_finite() || !t_end.is_finite() || !t_shift.is_finite() || t_shift <= 0.0 {
        return Err(OmniVoiceError::InvalidRequest(
            "timestep parameters must be finite and t_shift must be greater than zero".to_string(),
        ));
    }
    let capacity = num_step
        .checked_add(1)
        .ok_or_else(|| OmniVoiceError::InvalidRequest("num_step is too large".to_string()))?;
    let mut timesteps = Vec::with_capacity(capacity);
    for index in 0..=num_step {
        let t = t_start + ((t_end - t_start) * index as f32 / num_step as f32);
        let value = (t_shift * t) / (1.0 + (t_shift - 1.0) * t);
        if !value.is_finite() {
            return Err(OmniVoiceError::InvalidRequest(
                "timestep schedule contains a non-finite value".to_string(),
            ));
        }
        timesteps.push(value);
    }
    Ok(timesteps)
}

pub fn build_unmask_schedules(
    target_lens: &[usize],
    num_audio_codebook: usize,
    timesteps: &[f32],
    num_step: usize,
) -> Result<Vec<Vec<usize>>> {
    if num_step == 0 {
        return Err(OmniVoiceError::InvalidRequest(
            "num_step must be > 0".to_string(),
        ));
    }
    let expected_len = num_step
        .checked_add(1)
        .ok_or_else(|| OmniVoiceError::InvalidRequest("num_step is too large".to_string()))?;
    if timesteps.len() != expected_len {
        return Err(OmniVoiceError::InvalidRequest(format!(
            "timesteps length must be num_step + 1, got {} for num_step {num_step}",
            timesteps.len()
        )));
    }
    if timesteps.iter().any(|value| !value.is_finite()) {
        return Err(OmniVoiceError::InvalidRequest(
            "timesteps must contain only finite values".to_string(),
        ));
    }
    if timesteps.windows(2).any(|window| window[1] < window[0]) {
        return Err(OmniVoiceError::InvalidRequest(
            "timesteps must be non-decreasing".to_string(),
        ));
    }
    let mut schedules = Vec::with_capacity(target_lens.len());
    for target_len in target_lens {
        let total_mask = target_len.checked_mul(num_audio_codebook).ok_or_else(|| {
            OmniVoiceError::InvalidRequest("target token count is too large".to_string())
        })?;
        let mut remaining = total_mask;
        let mut schedule = Vec::with_capacity(num_step);
        for step in 0..num_step {
            let amount = if step == num_step - 1 {
                remaining
            } else {
                let delta = timesteps[step + 1] - timesteps[step];
                (total_mask as f32 * delta).ceil() as usize
            };
            let bounded = amount.min(remaining);
            schedule.push(bounded);
            remaining -= bounded;
        }
        schedules.push(schedule);
    }
    Ok(schedules)
}

pub fn pack_cfg_batch(prepared: &[PreparedPrompt], target_lens: &[usize]) -> Result<BatchedInputs> {
    if prepared.is_empty() {
        return Err(OmniVoiceError::InvalidRequest(
            "prepared prompts cannot be empty".to_string(),
        ));
    }
    if prepared.len() != target_lens.len() {
        return Err(OmniVoiceError::InvalidRequest(format!(
            "prepared prompt count {} does not match target lens {}",
            prepared.len(),
            target_lens.len()
        )));
    }

    let batch_size = prepared.len();
    let num_audio_codebook = prepared[0].prompt.input_ids.dims().1;
    if num_audio_codebook == 0 {
        return Err(OmniVoiceError::InvalidRequest(
            "prepared prompts must contain at least one audio codebook".to_string(),
        ));
    }
    for (index, (item, target_len)) in prepared.iter().zip(target_lens).enumerate() {
        let (input_batch, input_codebooks, input_len) = item.prompt.input_ids.dims();
        let (mask_batch, mask_len) = item.prompt.audio_mask.dims();
        if input_batch != 1
            || input_codebooks != num_audio_codebook
            || input_len != item.total_length
            || mask_batch != 1
            || mask_len != item.total_length
        {
            return Err(OmniVoiceError::InvalidData(format!(
                "prepared prompt {index} has inconsistent tensor dimensions"
            )));
        }
        if item.audio_mask_id != prepared[0].audio_mask_id {
            return Err(OmniVoiceError::InvalidData(format!(
                "prepared prompt {index} uses a different audio mask id"
            )));
        }
        if *target_len == 0 || *target_len > item.total_length {
            return Err(OmniVoiceError::InvalidRequest(format!(
                "target length at index {index} must be in 1..={}",
                item.total_length
            )));
        }
    }
    let max_c_len = prepared
        .iter()
        .map(|item| item.total_length)
        .max()
        .unwrap_or_default();
    let max_target_len = *target_lens.iter().max().unwrap_or(&0);
    let audio_mask_id = prepared[0].audio_mask_id;

    let mut batch_input_ids = I64Tensor3::full(
        (2 * batch_size, num_audio_codebook, max_c_len),
        audio_mask_id,
    );
    let mut batch_audio_mask = BoolTensor2::zeros((2 * batch_size, max_c_len));
    let mut batch_attention_mask = BoolTensor4::zeros((2 * batch_size, 1, max_c_len, max_c_len));
    let tokens_init = I64Tensor3::full(
        (batch_size, num_audio_codebook, max_target_len),
        audio_mask_id,
    );

    for (index, item) in prepared.iter().enumerate() {
        let cond_len = item.total_length;
        let target_len = target_lens[index];
        for codebook in 0..num_audio_codebook {
            for seq in 0..cond_len {
                let value = item.prompt.input_ids.get(0, codebook, seq);
                batch_input_ids.set(index, codebook, seq, value);
            }
        }
        for seq in 0..cond_len {
            let mask_value = item.prompt.audio_mask.get(0, seq);
            batch_audio_mask.set(index, seq, mask_value);
            for kv in 0..cond_len {
                batch_attention_mask.set(index, 0, seq, kv, true);
            }
        }

        let uncond_row = batch_size + index;
        for codebook in 0..num_audio_codebook {
            for seq in 0..target_len {
                let source = item
                    .prompt
                    .input_ids
                    .get(0, codebook, cond_len - target_len + seq);
                batch_input_ids.set(uncond_row, codebook, seq, source);
            }
        }
        for seq in 0..target_len {
            let source_mask = item.prompt.audio_mask.get(0, cond_len - target_len + seq);
            batch_audio_mask.set(uncond_row, seq, source_mask);
            for kv in 0..target_len {
                batch_attention_mask.set(uncond_row, 0, seq, kv, true);
            }
        }
        for diag in target_len..max_c_len {
            batch_attention_mask.set(uncond_row, 0, diag, diag, true);
        }
    }

    let timesteps = build_timesteps(0.0, 1.0, 32, 0.1)?;
    let schedules = build_unmask_schedules(target_lens, num_audio_codebook, &timesteps, 32)?;

    Ok(BatchedInputs {
        batch_input_ids,
        batch_audio_mask,
        batch_attention_mask,
        tokens_init,
        schedules,
    })
}

pub fn predict_tokens_with_scoring(
    c_logits: &[f32],
    u_logits: &[f32],
    guidance_scale: f32,
    class_temperature: f32,
    audio_mask_id: usize,
    num_audio_codebook: usize,
    audio_vocab_size: usize,
) -> Result<PredictionResult> {
    if c_logits.len() != u_logits.len() {
        return Err(OmniVoiceError::InvalidData(
            "c_logits and u_logits lengths differ".to_string(),
        ));
    }
    if num_audio_codebook == 0 || audio_vocab_size == 0 {
        return Err(OmniVoiceError::InvalidRequest(
            "num_audio_codebook and audio_vocab_size must be > 0".to_string(),
        ));
    }
    if audio_mask_id >= audio_vocab_size {
        return Err(OmniVoiceError::InvalidRequest(format!(
            "audio mask token {audio_mask_id} is outside vocab size {audio_vocab_size}"
        )));
    }
    if !guidance_scale.is_finite() || !class_temperature.is_finite() || class_temperature < 0.0 {
        return Err(OmniVoiceError::InvalidRequest(
            "guidance_scale and class_temperature must be finite, with class_temperature non-negative"
                .to_string(),
        ));
    }
    if !c_logits
        .len()
        .is_multiple_of(num_audio_codebook * audio_vocab_size)
    {
        return Err(OmniVoiceError::InvalidData(format!(
            "unexpected logits length {}",
            c_logits.len()
        )));
    }

    let position_count = c_logits.len() / (num_audio_codebook * audio_vocab_size);
    let mut pred_tokens = Vec::with_capacity(num_audio_codebook * position_count);
    let mut confidence_scores = Vec::with_capacity(num_audio_codebook * position_count);

    for layer in 0..num_audio_codebook {
        for position in 0..position_count {
            let start = (layer * position_count * audio_vocab_size) + (position * audio_vocab_size);
            let end = start + audio_vocab_size;
            let cond = log_softmax(&c_logits[start..end]);
            let uncond = log_softmax(&u_logits[start..end]);
            let mut combined = vec![0.0_f32; audio_vocab_size];
            for index in 0..audio_vocab_size {
                combined[index] = if guidance_scale != 0.0 {
                    cond[index] + guidance_scale * (cond[index] - uncond[index])
                } else {
                    cond[index]
                };
            }

            let normalized = log_softmax(&combined);
            let mut best_index = 0usize;
            let mut best_score = f32::NEG_INFINITY;
            for (index, score) in normalized.iter().enumerate() {
                if index == audio_mask_id {
                    continue;
                }
                // Temperature scaling preserves the argmax.  The live tensor
                // path adds Gumbel noise for non-zero temperatures; this
                // host-side helper has no RNG parameter, so keep its
                // deterministic fallback while retaining the unscaled
                // confidence used for position selection.
                if *score > best_score {
                    best_index = index;
                    best_score = *score;
                }
            }
            pred_tokens.push(best_index as i64);
            confidence_scores.push(best_score);
        }
    }

    Ok(PredictionResult {
        pred_tokens,
        confidence_scores,
        dims: (num_audio_codebook, position_count),
    })
}

pub fn apply_step_updates(
    current_tokens: &mut [i64],
    predicted_tokens: &[i64],
    scores: &[f32],
    mask_id: i64,
    update_count: usize,
) -> Result<()> {
    if current_tokens.len() != predicted_tokens.len() || current_tokens.len() != scores.len() {
        return Err(OmniVoiceError::InvalidData(
            "current_tokens, predicted_tokens, and scores must be same length".to_string(),
        ));
    }

    let mut candidates: Vec<(usize, f32)> = current_tokens
        .iter()
        .enumerate()
        .filter(|(_, token)| **token == mask_id)
        .map(|(index, _)| (index, scores[index]))
        .collect();
    candidates.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });

    for (index, _) in candidates.into_iter().take(update_count) {
        current_tokens[index] = predicted_tokens[index];
    }
    Ok(())
}

fn log_softmax(values: &[f32]) -> Vec<f32> {
    let max = values.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let sum = values.iter().map(|value| (*value - max).exp()).sum::<f32>();
    let log_sum = sum.ln();
    values.iter().map(|value| *value - max - log_sum).collect()
}
