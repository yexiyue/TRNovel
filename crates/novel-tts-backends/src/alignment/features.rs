//! Whisper-compatible 128-bin log-mel frontend, entirely native Rust.
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use rustfft::{FftPlanner, num_complex::Complex};
use tts_core::alignment::AudioClip;

pub(super) fn mono(audio: &AudioClip) -> anyhow::Result<Vec<f32>> {
    anyhow::ensure!(
        audio.channels > 0 && audio.sample_rate > 0,
        "invalid audio format"
    );
    let mut samples = Vec::new();
    for block in &audio.blocks {
        anyhow::ensure!(
            block.channels == audio.channels && block.sample_rate == audio.sample_rate,
            "PCM format changed"
        );
        for frame in block.samples.chunks_exact(audio.channels as usize) {
            samples.push(frame.iter().sum::<f32>() / audio.channels as f32);
        }
    }
    anyhow::ensure!(
        !samples.is_empty() && samples.iter().all(|x| x.is_finite()),
        "empty or invalid audio"
    );
    if audio.sample_rate == 16000 {
        return Ok(samples);
    }
    let ratio = 16000.0 / audio.sample_rate as f64;
    let mut resampler = SincFixedIn::<f32>::new(
        ratio,
        1.0,
        SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        },
        samples.len(),
        1,
    )?;
    let delay = resampler.output_delay();
    let length = (samples.len() as f64 * ratio).round() as usize;
    let mut output = resampler.process(&[samples], None)?.remove(0);
    output.extend(resampler.process_partial::<Vec<f32>>(None, None)?.remove(0));
    Ok(output.into_iter().skip(delay).take(length).collect())
}
fn hz_to_mel(hz: f64) -> f64 {
    if hz < 1000.0 {
        hz / (200.0 / 3.0)
    } else {
        15.0 + (hz / 1000.0).ln() / (6.4f64.ln() / 27.0)
    }
}
fn mel_to_hz(mel: f64) -> f64 {
    if mel < 15.0 {
        mel * (200.0 / 3.0)
    } else {
        1000.0 * ((mel - 15.0) * (6.4f64.ln() / 27.0)).exp()
    }
}
pub(super) fn log_mel(samples: &[f32]) -> anyhow::Result<(usize, Vec<f32>)> {
    anyhow::ensure!(
        samples.len() >= 400 && samples.len() <= 16000 * 30,
        "alignment audio must be 25 ms..30 s"
    );
    let frames = samples.len() / 160;
    let edges: Vec<f64> = (0..130)
        .map(|i| mel_to_hz(hz_to_mel(8000.0) * i as f64 / 129.0))
        .collect();
    let filters: Vec<Vec<f64>> = (0..128)
        .map(|m| {
            (0..201)
                .map(|k| {
                    let hz = k as f64 * 40.0;
                    ((hz - edges[m]) / (edges[m + 1] - edges[m]))
                        .min((edges[m + 2] - hz) / (edges[m + 2] - edges[m + 1]))
                        .max(0.0)
                        * 2.0
                        / (edges[m + 2] - edges[m])
                })
                .collect()
        })
        .collect();
    let window: Vec<f64> = (0..400)
        .map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / 400.0).cos())
        .collect();
    let fft = FftPlanner::new().plan_fft_forward(400);
    let mut spectrum = vec![Complex::default(); 400];
    let mut features = vec![0.0f64; 128 * frames];
    for frame in 0..frames {
        for i in 0..400 {
            let index = frame as isize * 160 + i as isize - 200;
            let reflected = if index < 0 {
                -index
            } else if index >= samples.len() as isize {
                2 * samples.len() as isize - 2 - index
            } else {
                index
            };
            spectrum[i] = Complex::new(samples[reflected as usize] as f64 * window[i], 0.0);
        }
        fft.process(&mut spectrum);
        for m in 0..128 {
            let power: f64 = filters[m]
                .iter()
                .zip(&spectrum)
                .map(|(weight, bin)| weight * bin.norm_sqr())
                .sum();
            features[m * frames + frame] = power.max(1e-10).log10();
        }
    }
    let floor = features.iter().copied().fold(f64::NEG_INFINITY, f64::max) - 8.0;
    Ok((
        frames,
        features
            .into_iter()
            .map(|x| ((x.max(floor) + 4.0) / 4.0) as f32)
            .collect(),
    ))
}
pub(super) fn audio_slots(frames: usize) -> usize {
    (frames / 100) * 13 + (frames % 100).div_ceil(8)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn mel_values_match_transformers_reference() {
        let audio: Vec<f32> = (0..16000).map(|i| (i as f32 * 0.1).sin() * 0.2).collect();
        let (_, mel) = log_mel(&audio).unwrap();
        let probes: serde_json::Value =
            serde_json::from_str(include_str!("../../tests/fixtures/qwen-mel.json")).unwrap();
        for probe in probes.as_array().unwrap() {
            let i = probe["index"].as_u64().unwrap() as usize;
            let expected = probe["value"].as_f64().unwrap() as f32;
            assert!(
                (mel[i] - expected).abs() < 0.0003,
                "mel {i}: {} versus {expected}",
                mel[i]
            );
        }
    }
    #[test]
    fn convolution_lengths_match_upstream() {
        assert_eq!(audio_slots(100), 13);
        assert_eq!(audio_slots(360), 47);
        assert_eq!(audio_slots(2000), 260);
    }
    #[test]
    fn silence_is_finite_and_shapes_are_bounded() {
        let (frames, features) = log_mel(&vec![0.0; 16000]).unwrap();
        assert_eq!(frames, 100);
        assert_eq!(features.len(), 12800);
        assert!(features.iter().all(|x| *x == -1.5));
    }
}
