//! WAV validation, channel conversion and band-limited resampling.
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::path::Path;
pub(super) fn read_wav(path: &Path) -> anyhow::Result<Vec<f32>> {
    let mut reader = hound::WavReader::open(path)?;
    let spec = reader.spec();
    anyhow::ensure!(
        (1..=2).contains(&spec.channels) && spec.sample_rate > 0,
        "WAV must be mono or stereo"
    );
    let duration = reader.duration() as f64 / spec.sample_rate as f64;
    anyhow::ensure!(
        (1.0..=30.0).contains(&duration),
        "reference audio must be 1..30 seconds"
    );
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().collect::<Result<_, _>>()?,
        hound::SampleFormat::Int => {
            anyhow::ensure!(
                spec.bits_per_sample > 0 && spec.bits_per_sample <= 32,
                "unsupported PCM depth"
            );
            let scale = 2f32.powi(spec.bits_per_sample as i32 - 1);
            reader
                .samples::<i32>()
                .map(|v| v.map(|v| v as f32 / scale))
                .collect::<Result<_, _>>()?
        }
    };
    anyhow::ensure!(
        samples.iter().all(|v| v.is_finite()) && samples.iter().any(|v| v.abs() > 0.0001),
        "reference audio is invalid or silent"
    );
    let mut channels = vec![Vec::new(), Vec::new()];
    for frame in samples.chunks_exact(spec.channels as usize) {
        channels[0].push(frame[0]);
        channels[1].push(*frame.get(1).unwrap_or(&frame[0]));
    }
    if spec.sample_rate != 48000 {
        let ratio = 48000. / spec.sample_rate as f64;
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };
        let mut resampler = SincFixedIn::<f32>::new(ratio, 1.0, params, channels[0].len(), 2)?;
        let delay = resampler.output_delay();
        let output = resampler.process(&channels, None)?;
        let tail = resampler.process_partial::<Vec<f32>>(None, None)?;
        let length = (channels[0].len() as f64 * ratio).round() as usize;
        channels = output
            .into_iter()
            .zip(tail)
            .map(|(mut main, tail)| {
                main.extend(tail);
                main.into_iter().skip(delay).take(length).collect()
            })
            .collect();
    }
    anyhow::ensure!(
        channels[0].len() == channels[1].len(),
        "invalid resampled waveform"
    );
    Ok(channels.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wav_resampling_and_silence_validation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("reference.wav");
        let spec = hound::WavSpec {
            channels: 1,
            sample_rate: 16000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        };
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for i in 0..16000 {
            writer
                .write_sample(((i as f32 * 0.05).sin() * 8000.) as i16)
                .unwrap();
        }
        writer.finalize().unwrap();
        let waveform = read_wav(&path).unwrap();
        assert_eq!(waveform.len(), 96000);
        assert_eq!(waveform[..48000], waveform[48000..]);
        let mut writer = hound::WavWriter::create(&path, spec).unwrap();
        for _ in 0..16000 {
            writer.write_sample(0i16).unwrap();
        }
        writer.finalize().unwrap();
        assert!(read_wav(&path).is_err());
    }
}
