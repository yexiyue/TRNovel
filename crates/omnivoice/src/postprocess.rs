use crate::error::{OmniVoiceError, Result};
use std::ops::Range;

const PCM16_SCALE: f32 = 32768.0;

pub fn fade_and_pad_audio(
    audio: &[f32],
    sample_rate: u32,
    pad_duration: f32,
    fade_duration: f32,
) -> Vec<f32> {
    if audio.is_empty() {
        return Vec::new();
    }

    let mut processed = audio.to_vec();
    let fade_samples = ((fade_duration * sample_rate as f32) as usize).min(processed.len() / 2);
    let pad_samples = (pad_duration * sample_rate as f32) as usize;

    if fade_samples > 0 {
        for (index, sample) in processed.iter_mut().enumerate().take(fade_samples) {
            *sample *= linspace_value(index, fade_samples, 0.0, 1.0);
        }
        let offset = processed.len() - fade_samples;
        for index in 0..fade_samples {
            processed[offset + index] *= linspace_value(index, fade_samples, 1.0, 0.0);
        }
    }

    if pad_samples > 0 {
        let mut padded = Vec::with_capacity(processed.len() + (2 * pad_samples));
        padded.resize(pad_samples, 0.0);
        padded.extend(processed);
        padded.resize(padded.len() + pad_samples, 0.0);
        padded
    } else {
        processed
    }
}

pub fn apply_clone_rms_restore(audio: &[f32], ref_rms: f32) -> Vec<f32> {
    if ref_rms >= 0.1 || audio.is_empty() {
        return audio.to_vec();
    }
    let gain = ref_rms / 0.1;
    audio.iter().map(|sample| sample * gain).collect()
}

pub fn peak_normalize_auto_voice(audio: &[f32]) -> Result<Vec<f32>> {
    if audio.is_empty() {
        return Ok(Vec::new());
    }
    let peak = audio
        .iter()
        .fold(0.0_f32, |max, sample| max.max(sample.abs()));
    if peak <= 1e-6 {
        return Ok(audio.to_vec());
    }
    Ok(audio.iter().map(|sample| sample / peak * 0.5).collect())
}

pub fn cross_fade_chunks(
    chunks: &[Vec<f32>],
    sample_rate: u32,
    silence_duration: f32,
) -> Result<Vec<f32>> {
    if chunks.is_empty() {
        return Ok(Vec::new());
    }
    if chunks.len() == 1 {
        return Ok(chunks[0].clone());
    }

    let total_gap = (silence_duration * sample_rate as f32) as usize;
    let fade_samples = total_gap / 3;
    let silence_samples = fade_samples;
    let mut merged = chunks[0].clone();

    for chunk in &chunks[1..] {
        let mut faded_current = merged;
        let fade_out = fade_samples.min(faded_current.len());
        if fade_out > 0 {
            let offset = faded_current.len() - fade_out;
            for index in 0..fade_out {
                faded_current[offset + index] *= linspace_value(index, fade_out, 1.0, 0.0);
            }
        }

        let mut faded_next = chunk.clone();
        let fade_in = fade_samples.min(faded_next.len());
        if fade_in > 0 {
            for (index, sample) in faded_next.iter_mut().enumerate().take(fade_in) {
                *sample *= linspace_value(index, fade_in, 0.0, 1.0);
            }
        }

        let mut next_merged =
            Vec::with_capacity(faded_current.len() + silence_samples + faded_next.len());
        next_merged.extend(faded_current);
        next_merged.resize(next_merged.len() + silence_samples, 0.0);
        next_merged.extend(faded_next);
        merged = next_merged;
    }

    Ok(merged)
}

pub fn remove_silence(
    audio: &[f32],
    sample_rate: u32,
    mid_sil: u32,
    lead_sil: u32,
    trail_sil: u32,
) -> Vec<f32> {
    let mut quantized = quantize_pcm16(audio);
    if mid_sil > 0 {
        let segments = split_on_silence(&quantized, sample_rate, mid_sil, -50.0, mid_sil, 10);
        quantized = if segments.is_empty() {
            Vec::new()
        } else {
            segments.into_iter().flatten().collect()
        };
    }
    let trimmed = remove_silence_edges_i16(&quantized, sample_rate, lead_sil, trail_sil, -50.0);
    dequantize_pcm16(&trimmed)
}

pub fn trim_edges_by_threshold(audio: &[f32], threshold: f32) -> Vec<f32> {
    if audio.is_empty() {
        return Vec::new();
    }
    let start = audio
        .iter()
        .position(|sample| sample.abs() > threshold)
        .unwrap_or(audio.len());
    let end = audio
        .iter()
        .rposition(|sample| sample.abs() > threshold)
        .map(|index| index + 1)
        .unwrap_or(start);
    if start >= end {
        Vec::new()
    } else {
        audio[start..end].to_vec()
    }
}

pub fn ensure_non_empty_audio(audio: &[f32]) -> Result<()> {
    if audio.is_empty() {
        Err(OmniVoiceError::InvalidData(
            "audio buffer is empty".to_string(),
        ))
    } else {
        Ok(())
    }
}

fn quantize_pcm16(audio: &[f32]) -> Vec<i16> {
    audio
        .iter()
        .map(|sample| {
            let scaled = sample.clamp(-1.0, 1.0) * PCM16_SCALE;
            scaled.clamp(i16::MIN as f32, i16::MAX as f32) as i16
        })
        .collect()
}

fn dequantize_pcm16(audio: &[i16]) -> Vec<f32> {
    audio
        .iter()
        .map(|sample| *sample as f32 / PCM16_SCALE)
        .collect()
}

fn split_on_silence(
    audio: &[i16],
    sample_rate: u32,
    min_silence_len_ms: u32,
    silence_threshold_db: f32,
    keep_silence_ms: u32,
    seek_step_ms: u32,
) -> Vec<Vec<i16>> {
    let nonsilent_ranges = detect_nonsilent(
        audio,
        sample_rate,
        min_silence_len_ms,
        silence_threshold_db,
        seek_step_ms,
    );
    if nonsilent_ranges.is_empty() {
        return Vec::new();
    }

    let total_ms = millis_from_samples(audio.len(), sample_rate);
    let mut ranges = nonsilent_ranges
        .into_iter()
        .map(|(start_ms, end_ms)| {
            (
                start_ms.saturating_sub(keep_silence_ms),
                total_ms.min(end_ms.saturating_add(keep_silence_ms)),
            )
        })
        .collect::<Vec<_>>();

    for index in 0..ranges.len().saturating_sub(1) {
        if ranges[index].1 > ranges[index + 1].0 {
            let midpoint = (ranges[index].1 + ranges[index + 1].0) / 2;
            ranges[index].1 = midpoint;
            ranges[index + 1].0 = midpoint;
        }
    }

    ranges
        .into_iter()
        .map(|(start_ms, end_ms)| {
            let range = clamped_sample_range_from_ms(start_ms, end_ms, sample_rate, audio.len());
            audio[range].to_vec()
        })
        .collect()
}

fn detect_nonsilent(
    audio: &[i16],
    sample_rate: u32,
    min_silence_len_ms: u32,
    silence_threshold_db: f32,
    seek_step_ms: u32,
) -> Vec<(u32, u32)> {
    let silent_ranges = detect_silence(
        audio,
        sample_rate,
        min_silence_len_ms,
        silence_threshold_db,
        seek_step_ms,
    );
    let total_ms = millis_from_samples(audio.len(), sample_rate);
    if silent_ranges.is_empty() {
        return vec![(0, total_ms)];
    }
    if silent_ranges.len() == 1 && silent_ranges[0].0 == 0 && silent_ranges[0].1 >= total_ms {
        return Vec::new();
    }

    let mut previous_end = 0_u32;
    let mut nonsilent = Vec::new();
    for (start_ms, end_ms) in silent_ranges {
        if start_ms > previous_end {
            nonsilent.push((previous_end, start_ms));
        }
        previous_end = end_ms;
    }
    if previous_end < total_ms {
        nonsilent.push((previous_end, total_ms));
    }
    nonsilent
}

fn detect_silence(
    audio: &[i16],
    sample_rate: u32,
    min_silence_len_ms: u32,
    silence_threshold_db: f32,
    seek_step_ms: u32,
) -> Vec<(u32, u32)> {
    let total_ms = millis_from_samples(audio.len(), sample_rate);
    if total_ms < min_silence_len_ms {
        return Vec::new();
    }

    let threshold = dbfs_threshold_i16(silence_threshold_db);
    let min_len = sample_count_from_ms(min_silence_len_ms, sample_rate).max(1);
    let step = sample_count_from_ms(seek_step_ms, sample_rate).max(1);
    let last_start = audio.len().saturating_sub(min_len);

    let mut starts = (0..=last_start).step_by(step).collect::<Vec<_>>();
    if starts.last().copied() != Some(last_start) {
        starts.push(last_start);
    }

    let mut silence_starts = Vec::new();
    for start in starts {
        if rms_pcm16(&audio[start..start + min_len]) <= threshold {
            silence_starts.push(start);
        }
    }
    if silence_starts.is_empty() {
        return Vec::new();
    }

    let mut ranges = Vec::new();
    let mut current = silence_starts[0];
    let mut previous = silence_starts[0];
    for start in silence_starts.iter().copied().skip(1) {
        let continuous = start == previous + step;
        let gap = start > previous + min_len;
        if !continuous && gap {
            ranges.push((current, previous + min_len));
            current = start;
        }
        previous = start;
    }
    ranges.push((current, previous + min_len));

    ranges
        .into_iter()
        .map(|(start, end)| {
            (
                millis_from_samples(start, sample_rate),
                millis_from_samples(end, sample_rate),
            )
        })
        .collect()
}

fn remove_silence_edges_i16(
    audio: &[i16],
    sample_rate: u32,
    lead_sil_ms: u32,
    trail_sil_ms: u32,
    silence_threshold_db: f32,
) -> Vec<i16> {
    let mut trimmed = audio.to_vec();
    let leading_ms = detect_leading_silence(&trimmed, sample_rate, silence_threshold_db, 10);
    let leading_start = clamped_sample_index_from_ms(
        leading_ms.saturating_sub(lead_sil_ms),
        sample_rate,
        trimmed.len(),
    );
    trimmed = trimmed[leading_start..].to_vec();

    trimmed.reverse();
    let trailing_ms = detect_leading_silence(&trimmed, sample_rate, silence_threshold_db, 10);
    let trailing_start = clamped_sample_index_from_ms(
        trailing_ms.saturating_sub(trail_sil_ms),
        sample_rate,
        trimmed.len(),
    );
    trimmed = trimmed[trailing_start..].to_vec();
    trimmed.reverse();
    trimmed
}

fn detect_leading_silence(
    audio: &[i16],
    sample_rate: u32,
    silence_threshold_db: f32,
    chunk_ms: u32,
) -> u32 {
    let threshold = dbfs_threshold_i16(silence_threshold_db);
    let chunk = sample_count_from_ms(chunk_ms, sample_rate).max(1);
    let mut trim = 0_usize;
    while trim < audio.len() {
        let end = (trim + chunk).min(audio.len());
        if rms_pcm16(&audio[trim..end]) > threshold {
            break;
        }
        trim += chunk;
    }
    millis_from_samples(trim, sample_rate)
}

fn rms_pcm16(audio: &[i16]) -> f32 {
    if audio.is_empty() {
        return 0.0;
    }
    let mean_square = audio
        .iter()
        .map(|sample| {
            let value = *sample as f64;
            value * value
        })
        .sum::<f64>()
        / audio.len() as f64;
    mean_square.sqrt() as f32
}

fn dbfs_threshold_i16(db: f32) -> f32 {
    10_f32.powf(db / 20.0) * PCM16_SCALE
}

fn sample_count_from_ms(duration_ms: u32, sample_rate: u32) -> usize {
    ((duration_ms as u64 * sample_rate as u64) / 1000) as usize
}

fn sample_index_from_ms(duration_ms: u32, sample_rate: u32) -> usize {
    sample_count_from_ms(duration_ms, sample_rate)
}

fn clamped_sample_index_from_ms(duration_ms: u32, sample_rate: u32, audio_len: usize) -> usize {
    sample_index_from_ms(duration_ms, sample_rate).min(audio_len)
}

fn clamped_sample_range_from_ms(
    start_ms: u32,
    end_ms: u32,
    sample_rate: u32,
    audio_len: usize,
) -> Range<usize> {
    let end = clamped_sample_index_from_ms(end_ms, sample_rate, audio_len);
    let start = clamped_sample_index_from_ms(start_ms, sample_rate, audio_len).min(end);
    start..end
}

fn millis_from_samples(sample_count: usize, sample_rate: u32) -> u32 {
    (((sample_count as u64 * 1000) + (sample_rate as u64 / 2)) / sample_rate as u64) as u32
}

fn linspace_value(index: usize, count: usize, start: f32, end: f32) -> f32 {
    if count <= 1 {
        return start;
    }
    start + ((end - start) * index as f32 / (count - 1) as f32)
}
