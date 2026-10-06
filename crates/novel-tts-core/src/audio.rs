//! Streaming boundary-silence processing; interior pauses remain unchanged.
use crate::backend::{BackendError, Pcm};
#[derive(Default)]
pub struct BoundarySilence {
    format: Option<(u32, u16)>,
    remainder: Vec<f32>,
    quiet: Vec<f32>,
    started: bool,
}
impl BoundarySilence {
    pub fn push(&mut self, audio: Pcm) -> Result<Option<Pcm>, BackendError> {
        audio.duration_ms()?;
        let format = (audio.sample_rate, audio.channels);
        if self.format.is_some_and(|old| old != format) {
            return Err(BackendError::Unsupported("PCM format changed".into()));
        }
        self.format = Some(format);
        self.remainder.extend(audio.samples);
        let window = ((audio.sample_rate / 100) as usize).max(1) * audio.channels as usize;
        let consumed = self.remainder.len() / window * window;
        let remainder = self.remainder.split_off(consumed);
        let windows = std::mem::replace(&mut self.remainder, remainder);
        let mut output = Vec::new();
        for samples in windows.chunks_exact(window) {
            self.window(samples, &mut output)?;
        }
        Ok(self.pcm(output))
    }
    fn window(&mut self, samples: &[f32], output: &mut Vec<f32>) -> Result<(), BackendError> {
        let peak = samples
            .iter()
            .fold(0f32, |peak, value| peak.max(value.abs()));
        let rms = (samples.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / samples.len() as f64)
            .sqrt();
        if peak <= 10f32.powf(-45.0 / 20.0) && rms <= 10f64.powf(-55.0 / 20.0) {
            self.quiet.extend_from_slice(samples);
            if !self.started {
                let (rate, channels) = self.format.unwrap();
                let keep = rate as usize * channels as usize / 10;
                if self.quiet.len() > keep {
                    self.quiet.drain(..self.quiet.len() - keep);
                }
            }
            if self.quiet.len() > 16 * 1024 * 1024 / size_of::<f32>() {
                return Err(BackendError::Unsupported(
                    "silence exceeds audio budget".into(),
                ));
            }
        } else {
            output.append(&mut self.quiet);
            output.extend_from_slice(samples);
            self.started = true;
        }
        Ok(())
    }
    pub fn finish(&mut self, paragraph_end: bool) -> Result<Option<Pcm>, BackendError> {
        if self.format.is_none() {
            return Err(BackendError::Unsupported("empty audio stream".into()));
        }
        let tail = std::mem::take(&mut self.remainder);
        let mut output = Vec::new();
        if !tail.is_empty() {
            self.window(&tail, &mut output)?;
        }
        if !self.started {
            return Err(BackendError::Unsupported(
                "backend produced only silence".into(),
            ));
        }
        let (rate, channels) = self.format.unwrap();
        let keep = rate as usize * channels as usize * if paragraph_end { 300 } else { 150 } / 1000;
        output.extend(self.quiet.drain(..self.quiet.len().min(keep)));
        self.quiet.clear();
        Ok(self.pcm(output))
    }
    fn pcm(&self, samples: Vec<f32>) -> Option<Pcm> {
        let (sample_rate, channels) = self.format?;
        (!samples.is_empty()).then_some(Pcm {
            samples,
            sample_rate,
            channels,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn pcm(samples: Vec<f32>) -> Pcm {
        Pcm {
            samples,
            sample_rate: 1000,
            channels: 1,
        }
    }
    #[test]
    fn preserves_interior_pause_and_caps_only_boundaries() {
        let samples = [
            vec![0.0; 500],
            vec![0.2; 100],
            vec![0.0; 400],
            vec![0.2; 100],
            vec![0.0; 700],
        ]
        .concat();
        let mut filter = BoundarySilence::default();
        let mut output = Vec::new();
        for chunk in samples.chunks(73) {
            if let Some(audio) = filter.push(pcm(chunk.to_vec())).unwrap() {
                output.extend(audio.samples);
            }
        }
        output.extend(filter.finish(false).unwrap().unwrap().samples);
        assert_eq!(output.len(), 100 + 100 + 400 + 100 + 150);
        assert_eq!(&output[200..600], &[0.0; 400]);
    }
}
