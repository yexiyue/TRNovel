use std::{
    fs::File,
    io::{Cursor, ErrorKind, Read, Seek},
    path::Path,
};

use symphonia::core::{
    audio::SampleBuffer, codecs::DecoderOptions, errors::Error as SymphoniaError,
    formats::FormatOptions, io::MediaSourceStream, meta::MetadataOptions, probe::Hint,
};

use crate::{
    contracts::{ReferenceAudioInput, WaveformInput},
    error::{OmniVoiceError, Result},
    frontend::add_punctuation,
    postprocess::remove_silence,
};

#[derive(Clone, Debug, PartialEq)]
pub struct PreparedReferenceAudio {
    pub waveform: Vec<f32>,
    pub ref_rms: Option<f32>,
    pub ref_text: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceAudioProcessor {
    target_sample_rate: u32,
    hop_length: usize,
}

impl ReferenceAudioProcessor {
    pub fn new(target_sample_rate: u32, hop_length: usize) -> Self {
        Self {
            target_sample_rate,
            hop_length,
        }
    }

    pub fn load_input(&self, input: &ReferenceAudioInput) -> Result<WaveformInput> {
        let waveform = match input {
            ReferenceAudioInput::FilePath(path) => load_audio_file(path),
            ReferenceAudioInput::Waveform(waveform) => Ok(waveform.clone()),
        }?;
        validate_waveform(&waveform)?;
        if self.target_sample_rate == 0 {
            return Err(OmniVoiceError::InvalidRequest(
                "target audio sample rate must be greater than zero".to_string(),
            ));
        }
        if self.hop_length == 0 {
            return Err(OmniVoiceError::InvalidRequest(
                "audio hop length must be greater than zero".to_string(),
            ));
        }
        Ok(waveform)
    }

    pub fn prepare_prompt_audio(
        &self,
        input: &ReferenceAudioInput,
        ref_text: Option<&str>,
        preprocess_prompt: bool,
    ) -> Result<PreparedReferenceAudio> {
        let waveform = self.load_input(input)?;
        let mut mono = mono_samples(&waveform.samples, waveform.channels)?;
        if waveform.sample_rate != self.target_sample_rate {
            mono = resample_linear(&mono, waveform.sample_rate, self.target_sample_rate);
        }

        let ref_rms = root_mean_square(&mono);
        if ref_rms.is_some_and(|rms| !rms.is_finite()) {
            return Err(OmniVoiceError::InvalidRequest(
                "reference audio RMS is not finite".to_string(),
            ));
        }
        if let Some(rms) = ref_rms
            && rms > 0.0
            && rms < 0.1
        {
            let scale = 0.1 / rms;
            for sample in &mut mono {
                *sample *= scale;
            }
        }

        if preprocess_prompt {
            if ref_text.is_none() {
                mono = trim_long_audio(&mono, self.target_sample_rate, 15.0, 3.0, 20.0);
            }
            mono = remove_silence(&mono, self.target_sample_rate, 200, 100, 200);
            if mono.is_empty() {
                return Err(OmniVoiceError::InvalidRequest(
                    "reference audio is empty after silence removal".to_string(),
                ));
            }
        }

        mono = trim_to_hop_multiple(&mono, self.hop_length);
        if mono.is_empty() {
            return Err(OmniVoiceError::InvalidRequest(
                "reference audio is too short after hop alignment".to_string(),
            ));
        }
        let ref_text = ref_text.map(|text| {
            if preprocess_prompt {
                add_punctuation(text)
            } else {
                text.to_string()
            }
        });

        Ok(PreparedReferenceAudio {
            waveform: mono,
            ref_rms,
            ref_text,
        })
    }
}

pub fn load_audio_file(path: impl AsRef<Path>) -> Result<WaveformInput> {
    let path = path.as_ref();
    match load_audio_file_symphonia(path) {
        Ok(waveform) => Ok(waveform),
        Err(primary) => {
            let is_wav = path
                .extension()
                .and_then(|value| value.to_str())
                .map(|value| value.eq_ignore_ascii_case("wav"))
                .unwrap_or(false);
            if is_wav {
                load_wave_file(path).map_err(|fallback| {
                    OmniVoiceError::InvalidData(format!(
                        "failed to decode audio file {} via symphonia ({primary}); wav fallback failed: {fallback}",
                        path.display()
                    ))
                })
            } else {
                Err(primary)
            }
        }
    }
}

pub fn load_audio_bytes(bytes: &[u8], extension_hint: Option<&str>) -> Result<WaveformInput> {
    match load_audio_bytes_symphonia(bytes, extension_hint) {
        Ok(waveform) => Ok(waveform),
        Err(primary) => {
            let is_wav = extension_hint
                .map(|value| value.eq_ignore_ascii_case("wav"))
                .unwrap_or(false);
            if is_wav {
                load_wave_reader(Cursor::new(bytes.to_vec())).map_err(|fallback| {
                    OmniVoiceError::InvalidData(format!(
                        "failed to decode audio bytes via symphonia ({primary}); wav fallback failed: {fallback}",
                    ))
                })
            } else {
                Err(primary)
            }
        }
    }
}

fn load_audio_file_symphonia(path: &Path) -> Result<WaveformInput> {
    let file = File::open(path)?;
    let media_source = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = path.extension().and_then(|value| value.to_str()) {
        hint.with_extension(extension);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            media_source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| {
            OmniVoiceError::InvalidData(format!(
                "symphonia could not probe {}: {error}",
                path.display()
            ))
        })?;
    load_audio_from_probed(path.display().to_string(), probed)
}

fn load_audio_bytes_symphonia(bytes: &[u8], extension_hint: Option<&str>) -> Result<WaveformInput> {
    let cursor = Cursor::new(bytes.to_vec());
    let media_source = MediaSourceStream::new(Box::new(cursor), Default::default());
    let mut hint = Hint::new();
    if let Some(extension) = extension_hint {
        hint.with_extension(extension);
    }
    let label = extension_hint
        .map(|extension| format!("memory.{extension}"))
        .unwrap_or_else(|| "memory.audio".to_string());
    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            media_source,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map_err(|error| {
            OmniVoiceError::InvalidData(format!(
                "symphonia could not probe in-memory audio payload {label}: {error}",
            ))
        })?;
    load_audio_from_probed(label, probed)
}

fn load_audio_from_probed(
    label: String,
    probed: symphonia::core::probe::ProbeResult,
) -> Result<WaveformInput> {
    let mut format = probed.format;
    let track = format
        .default_track()
        .ok_or_else(|| OmniVoiceError::InvalidData(format!("no default audio track in {label}")))?;
    let track_id = track.id;
    let mut sample_rate = track.codec_params.sample_rate;
    let mut channels = track.codec_params.channels.map(|value| value.count());
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|error| {
            OmniVoiceError::InvalidData(format!(
                "symphonia could not create decoder for {label}: {error}",
            ))
        })?;
    let mut samples = Vec::new();

    loop {
        let packet = match format.next_packet() {
            Ok(packet) => packet,
            Err(SymphoniaError::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::ResetRequired) => {
                return Err(OmniVoiceError::Unsupported(format!(
                    "symphonia stream reset is unsupported for {label}",
                )));
            }
            Err(error) => {
                return Err(OmniVoiceError::InvalidData(format!(
                    "symphonia failed reading packet from {label}: {error}",
                )));
            }
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(decoded) => decoded,
            Err(SymphoniaError::IoError(error)) if error.kind() == ErrorKind::UnexpectedEof => {
                break;
            }
            Err(SymphoniaError::DecodeError(error)) => {
                return Err(OmniVoiceError::InvalidData(format!(
                    "symphonia decode error for {label}: {error}",
                )));
            }
            Err(SymphoniaError::ResetRequired) => {
                return Err(OmniVoiceError::Unsupported(format!(
                    "symphonia decoder reset is unsupported for {label}",
                )));
            }
            Err(error) => {
                return Err(OmniVoiceError::InvalidData(format!(
                    "symphonia failed decoding {label}: {error}",
                )));
            }
        };

        sample_rate.get_or_insert(decoded.spec().rate);
        channels.get_or_insert(decoded.spec().channels.count());
        let mut buffer = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        buffer.copy_interleaved_ref(decoded);
        samples.extend_from_slice(buffer.samples());
    }

    let sample_rate = sample_rate.ok_or_else(|| {
        OmniVoiceError::InvalidData(format!("missing sample rate metadata in {label}"))
    })?;
    let channels = channels.ok_or_else(|| {
        OmniVoiceError::InvalidData(format!("missing channel metadata in {label}"))
    })?;

    if samples.is_empty() {
        return Err(OmniVoiceError::InvalidData(format!(
            "decoded audio stream is empty in {label}",
        )));
    }

    let waveform = WaveformInput {
        samples,
        sample_rate,
        channels,
    };
    validate_waveform(&waveform)?;
    Ok(waveform)
}

pub fn load_wave_file(path: impl AsRef<Path>) -> Result<WaveformInput> {
    let path = path.as_ref();
    let file = File::open(path)?;
    load_wave_reader(file)
}

fn load_wave_reader<R>(reader: R) -> Result<WaveformInput>
where
    R: Read + Seek,
{
    let mut reader = hound::WavReader::new(reader)?;
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
    let waveform = WaveformInput {
        samples,
        sample_rate: spec.sample_rate,
        channels: spec.channels as usize,
    };
    validate_waveform(&waveform)?;
    Ok(waveform)
}

pub fn mono_samples(samples: &[f32], channels: usize) -> Result<Vec<f32>> {
    if channels == 0 {
        return Err(OmniVoiceError::InvalidRequest(
            "audio channel count must be greater than zero".to_string(),
        ));
    }
    if channels == 1 {
        return Ok(samples.to_vec());
    }
    if !samples.len().is_multiple_of(channels) {
        return Err(OmniVoiceError::InvalidRequest(format!(
            "audio sample count {} is not divisible by channel count {channels}",
            samples.len()
        )));
    }
    let mut mono = vec![0.0; samples.len() / channels];
    for (index, sample) in samples.iter().enumerate() {
        mono[index / channels] += *sample;
    }
    for sample in &mut mono {
        *sample /= channels as f32;
    }
    Ok(mono)
}

pub fn trim_to_hop_multiple(samples: &[f32], hop_length: usize) -> Vec<f32> {
    if hop_length == 0 {
        return samples.to_vec();
    }
    let remainder = samples.len() % hop_length;
    if remainder == 0 {
        samples.to_vec()
    } else {
        samples[..samples.len() - remainder].to_vec()
    }
}

pub fn root_mean_square(samples: &[f32]) -> Option<f32> {
    if samples.is_empty() {
        return None;
    }
    let energy = samples
        .iter()
        .map(|sample| f64::from(*sample) * f64::from(*sample))
        .sum::<f64>()
        / samples.len() as f64;
    let rms = energy.sqrt();
    Some(rms as f32)
}

pub fn resample_linear(samples: &[f32], from_sample_rate: u32, to_sample_rate: u32) -> Vec<f32> {
    if samples.is_empty()
        || from_sample_rate == 0
        || to_sample_rate == 0
        || from_sample_rate == to_sample_rate
    {
        return samples.to_vec();
    }
    let ratio = to_sample_rate as f64 / from_sample_rate as f64;
    let output_len = ((samples.len() as f64 * ratio).round() as usize).max(1);
    let cutoff = ratio.min(1.0);
    let radius = 16.0_f64;
    let mut output = Vec::with_capacity(output_len);
    for output_index in 0..output_len {
        let center = output_index as f64 / ratio;
        let first = (center.floor() as isize - radius as isize + 1).max(0) as usize;
        let last = (center.ceil() as isize + radius as isize)
            .min(samples.len() as isize - 1)
            .max(first as isize) as usize;
        let mut value = 0.0_f64;
        let mut weight_sum = 0.0_f64;
        for (offset, sample) in samples[first..=last].iter().enumerate() {
            let index = first + offset;
            let distance = center - index as f64;
            if distance.abs() >= radius {
                continue;
            }
            let scaled = cutoff * distance;
            let sinc = if scaled.abs() < f64::EPSILON {
                1.0
            } else {
                let pi_scaled = std::f64::consts::PI * scaled;
                pi_scaled.sin() / pi_scaled
            };
            let window = 0.5 * (1.0 + (std::f64::consts::PI * distance / radius).cos());
            let weight = cutoff * sinc * window;
            value += *sample as f64 * weight;
            weight_sum += weight;
        }
        output.push(if weight_sum.abs() > f64::EPSILON {
            (value / weight_sum) as f32
        } else {
            samples[center.round().min((samples.len() - 1) as f64) as usize]
        });
    }
    output
}

pub fn trim_long_audio(
    samples: &[f32],
    sample_rate: u32,
    max_duration: f32,
    min_duration: f32,
    trim_threshold: f32,
) -> Vec<f32> {
    if sample_rate == 0 {
        return samples.to_vec();
    }
    let duration = samples.len() as f32 / sample_rate as f32;
    if duration <= trim_threshold {
        return samples.to_vec();
    }

    let quantized = quantize_pcm16(samples);
    let nonsilent = detect_nonsilent_ranges_pcm16(&quantized, sample_rate, 100, -40.0, 10);
    if nonsilent.is_empty() {
        return samples.to_vec();
    }

    let total_ms = millis_from_samples(samples.len(), sample_rate);
    let max_ms = ((max_duration * 1000.0).round() as u32).min(total_ms);
    let min_ms = ((min_duration * 1000.0).round() as u32).min(max_ms);
    let mut best_split_ms = 0_u32;

    for (start_ms, end_ms) in nonsilent {
        if start_ms > best_split_ms && start_ms <= max_ms {
            best_split_ms = start_ms;
        }
        if end_ms > max_ms {
            break;
        }
    }

    if best_split_ms < min_ms {
        best_split_ms = max_ms;
    }
    let split_index = sample_count_from_ms(best_split_ms, sample_rate).min(samples.len());
    samples[..split_index].to_vec()
}

fn db_to_amplitude(db: f32) -> f32 {
    10f32.powf(db / 20.0)
}

fn quantize_pcm16(samples: &[f32]) -> Vec<i16> {
    samples
        .iter()
        .map(|sample| {
            let scaled = sample.clamp(-1.0, 1.0) * 32768.0;
            scaled.clamp(i16::MIN as f32, i16::MAX as f32) as i16
        })
        .collect()
}

fn detect_nonsilent_ranges_pcm16(
    audio: &[i16],
    sample_rate: u32,
    min_silence_len_ms: u32,
    silence_threshold_db: f32,
    seek_step_ms: u32,
) -> Vec<(u32, u32)> {
    let silent_ranges = detect_silence_ranges_pcm16(
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

fn detect_silence_ranges_pcm16(
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

    let threshold = db_to_amplitude(silence_threshold_db) * 32768.0;
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

fn sample_count_from_ms(duration_ms: u32, sample_rate: u32) -> usize {
    ((duration_ms as u64 * sample_rate as u64) / 1000) as usize
}

fn millis_from_samples(sample_count: usize, sample_rate: u32) -> u32 {
    if sample_rate == 0 {
        return 0;
    }
    (((sample_count as u64 * 1000) + (sample_rate as u64 / 2)) / sample_rate as u64) as u32
}

fn validate_waveform(waveform: &WaveformInput) -> Result<()> {
    if waveform.sample_rate == 0 {
        return Err(OmniVoiceError::InvalidRequest(
            "audio sample rate must be greater than zero".to_string(),
        ));
    }
    if waveform.channels == 0 {
        return Err(OmniVoiceError::InvalidRequest(
            "audio channel count must be greater than zero".to_string(),
        ));
    }
    if !waveform.samples.len().is_multiple_of(waveform.channels) {
        return Err(OmniVoiceError::InvalidRequest(format!(
            "audio sample count {} is not divisible by channel count {}",
            waveform.samples.len(),
            waveform.channels
        )));
    }
    if let Some(index) = waveform
        .samples
        .iter()
        .position(|sample| !sample.is_finite())
    {
        return Err(OmniVoiceError::InvalidRequest(format!(
            "audio sample at index {index} is not finite"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{load_audio_file, mono_samples, trim_long_audio};

    #[test]
    fn generic_audio_loader_decodes_reference_wav() {
        let path = crate::paths::ref_audio_path();
        if !path.is_file() {
            return;
        }
        let waveform = load_audio_file(path).unwrap();
        assert!(waveform.sample_rate > 0);
        assert!(waveform.channels >= 1);
        assert!(!waveform.samples.is_empty());
    }

    #[test]
    fn trim_long_audio_prefers_latest_silence_gap_before_max_duration() {
        let sample_rate = 1000;
        let mut samples = vec![0.5; 2500];
        samples.extend(vec![0.0; 200]);
        samples.extend(vec![0.5; 2500]);

        let trimmed = trim_long_audio(&samples, sample_rate, 3.0, 1.0, 4.0);

        assert_eq!(trimmed.len(), 2700);
    }

    #[test]
    fn mono_samples_rejects_invalid_channel_layout() {
        assert!(mono_samples(&[0.0], 0).is_err());
        assert!(mono_samples(&[0.0, 1.0, 2.0], 2).is_err());
    }

    #[test]
    fn trim_to_hop_multiple_handles_zero_hop_without_panicking() {
        assert_eq!(super::trim_to_hop_multiple(&[1.0, 2.0], 0), vec![1.0, 2.0]);
    }
}
