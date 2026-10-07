//! Vocos' periodic Hann STFT, HTK magnitude mel bank and centered inverse STFT.
use rustfft::{FftPlanner, num_complex::Complex32};
const FFT: usize = 1024;
const HOP: usize = 256;
pub const MELS: usize = 100;
fn window(i: usize) -> f32 {
    0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / FFT as f32).cos()
}
fn reflected(index: isize, length: usize) -> usize {
    let period = 2 * (length - 1) as isize;
    let value = index.rem_euclid(period) as usize;
    if value < length {
        value
    } else {
        period as usize - value
    }
}
pub fn features(samples: &[f32]) -> anyhow::Result<Vec<f32>> {
    anyhow::ensure!(
        samples.len() > FFT / 2 && samples.iter().all(|v| v.is_finite()),
        "invalid reference waveform"
    );
    let frames = (samples.len() as f64 / HOP as f64 + 0.5).floor() as usize;
    let mel_max = 2595.0_f64 * (1.0 + 12000.0_f64 / 700.0).log10();
    let frequencies: Vec<_> = (0..MELS + 2)
        .map(|i| 700.0 * (10.0_f64.powf(mel_max * i as f64 / (MELS + 1) as f64 / 2595.0) - 1.0))
        .collect();
    let fft = FftPlanner::new().plan_fft_forward(FFT);
    let mut buffer = vec![Complex32::default(); FFT];
    let mut result = Vec::with_capacity(frames * MELS);
    for frame in 0..frames {
        for (i, value) in buffer.iter_mut().enumerate() {
            let index = frame as isize * HOP as isize + i as isize - (FFT / 2) as isize;
            *value = Complex32::new(samples[reflected(index, samples.len())] * window(i), 0.0);
        }
        fft.process(&mut buffer);
        for mel in 0..MELS {
            let mut magnitude = 0.0;
            for (bin, value) in buffer[..=FFT / 2].iter().enumerate() {
                let hz = bin as f64 * 24000.0 / FFT as f64;
                let weight = ((hz - frequencies[mel]) / (frequencies[mel + 1] - frequencies[mel]))
                    .min(
                        (frequencies[mel + 2] - hz) / (frequencies[mel + 2] - frequencies[mel + 1]),
                    )
                    .max(0.0);
                magnitude += value.norm() * weight as f32;
            }
            result.push(magnitude.max(1e-7).ln() * 0.1);
        }
    }
    Ok(result)
}
pub fn waveform(
    magnitude: &[f32],
    real: &[f32],
    imaginary: &[f32],
    frames: usize,
) -> anyhow::Result<Vec<f32>> {
    anyhow::ensure!(
        frames > 1
            && [magnitude, real, imaginary]
                .iter()
                .all(|v| v.len() == 513 * frames && v.iter().all(|x| x.is_finite())),
        "invalid Vocos spectral output"
    );
    let length = FFT + HOP * (frames - 1);
    let mut output = vec![0.0; length];
    let mut weights = vec![0.0; length];
    let mut spectrum = vec![Complex32::default(); FFT];
    let fft = FftPlanner::new().plan_fft_inverse(FFT);
    for frame in 0..frames {
        for (bin, value) in spectrum[..=FFT / 2].iter_mut().enumerate() {
            let index = bin * frames + frame;
            *value = Complex32::new(real[index], imaginary[index]) * magnitude[index];
        }
        for bin in 1..FFT / 2 {
            spectrum[FFT - bin] = spectrum[bin].conj();
        }
        fft.process(&mut spectrum);
        for (i, value) in spectrum.iter().enumerate() {
            let index = frame * HOP + i;
            let hann = window(i);
            output[index] += value.re * hann / FFT as f32;
            weights[index] += hann * hann;
        }
    }
    Ok((FFT / 2..length - FFT / 2)
        .map(|i| output[i] / weights[i])
        .collect())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(serde::Deserialize)]
    struct Golden {
        samples: Vec<f32>,
        features: Vec<f32>,
        magnitude: Vec<f32>,
        real: Vec<f32>,
        imaginary: Vec<f32>,
        frames: usize,
        waveform: Vec<f32>,
    }
    #[test]
    fn matches_torchaudio_magnitude_mels_and_torch_centered_istft() {
        let fixture: Golden = serde_json::from_str(include_str!("audio-golden.json")).unwrap();
        let mel = features(&fixture.samples).unwrap();
        assert_eq!(mel.len(), fixture.features.len());
        let max_error = mel
            .iter()
            .zip(&fixture.features)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(max_error < 0.002, "mel max error {max_error}");
        let pcm = waveform(
            &fixture.magnitude,
            &fixture.real,
            &fixture.imaginary,
            fixture.frames,
        )
        .unwrap();
        assert_eq!(pcm.len(), fixture.waveform.len());
        let max_error = pcm
            .iter()
            .zip(&fixture.waveform)
            .map(|(a, b)| (a - b).abs())
            .fold(0.0_f32, f32::max);
        assert!(max_error < 1e-6, "inverse STFT max error {max_error}");
    }
}
