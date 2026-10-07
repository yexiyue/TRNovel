//! Shared WAV validation and mono resampling for reference-based adapters.
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::path::Path;
pub fn load(path: &Path, target_rate: u32) -> anyhow::Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    anyhow::ensure!(
        (1..=2).contains(&spec.channels) && (8000..=192000).contains(&spec.sample_rate),
        "reference WAV must be mono/stereo at 8..192 kHz"
    );
    let duration = reader.duration() as f64 / spec.sample_rate as f64;
    anyhow::ensure!(
        (1.0..=30.0).contains(&duration),
        "reference WAV must contain 1..30 seconds"
    );
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<Vec<_>, _>>()?,
        hound::SampleFormat::Int => {
            anyhow::ensure!(
                (1..=32).contains(&spec.bits_per_sample),
                "unsupported reference PCM depth"
            );
            let scale = 2f32.powi(spec.bits_per_sample as i32 - 1);
            reader
                .samples::<i32>()
                .map(|v| v.map(|v| v as f32 / scale))
                .collect::<Result<Vec<_>, _>>()?
        }
    };
    anyhow::ensure!(
        samples.len().is_multiple_of(spec.channels as usize)
            && samples.iter().all(|v| v.is_finite()),
        "reference WAV is invalid or silent"
    );
    let mono: Vec<_> = samples
        .chunks_exact(spec.channels as usize)
        .map(|frame| frame.iter().sum::<f32>() / spec.channels as f32)
        .collect();
    anyhow::ensure!(
        mono.iter().all(|v| v.is_finite()) && mono.iter().any(|v| v.abs() > 0.0001),
        "reference WAV is invalid or silent after mono conversion"
    );
    if spec.sample_rate == target_rate {
        return Ok(mono);
    }
    let ratio = target_rate as f64 / spec.sample_rate as f64;
    let params = SincInterpolationParameters {
        sinc_len: 256,
        f_cutoff: 0.95,
        interpolation: SincInterpolationType::Linear,
        oversampling_factor: 256,
        window: WindowFunction::BlackmanHarris2,
    };
    let mut resampler = SincFixedIn::<f32>::new(ratio, 1.0, params, mono.len(), 1)?;
    let delay = resampler.output_delay();
    let length = (mono.len() as f64 * ratio).round() as usize;
    let mut output = resampler.process(&[mono], None)?.remove(0);
    output.extend(resampler.process_partial::<Vec<f32>>(None, None)?.remove(0));
    let output: Vec<_> = output.into_iter().skip(delay).take(length).collect();
    anyhow::ensure!(
        output.len() == length && output.iter().all(|v| v.is_finite()),
        "invalid resampled reference"
    );
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wav(path: &Path, rate: u32, channels: u16, samples: impl IntoIterator<Item = f32>) {
        let mut writer = hound::WavWriter::create(
            path,
            hound::WavSpec {
                channels,
                sample_rate: rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .unwrap();
        for sample in samples {
            writer.write_sample(sample).unwrap();
        }
        writer.finalize().unwrap();
    }
    #[test]
    fn resampling_preserves_duration_and_rejects_nonfinite_or_silent_mono() {
        let temporary = tempfile::tempdir().unwrap();
        let path = temporary.path().join("reference.wav");
        wav(
            &path,
            48000,
            1,
            (0..48000).map(|i| 0.1 * (std::f32::consts::TAU * 500.0 * i as f32 / 48000.0).sin()),
        );
        let samples = load(&path, 16000).unwrap();
        assert_eq!(samples.len(), 16000);
        let rms = (samples.iter().map(|v| v * v).sum::<f32>() / samples.len() as f32).sqrt();
        assert!((0.069..0.073).contains(&rms));
        wav(&path, 24000, 2, (0..24000).flat_map(|_| [0.1, -0.1]));
        assert!(load(&path, 24000).is_err());
        wav(&path, 24000, 1, std::iter::repeat_n(f32::NAN, 24000));
        assert!(load(&path, 24000).is_err());
    }
}
