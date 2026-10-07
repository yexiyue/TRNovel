use super::{audio, frontend::Frontend};
use ort::{session::Session, value::Tensor};
use rand::SeedableRng;
use rand_distr::{Distribution, StandardNormal};
use std::path::Path;
pub struct Model {
    encoder: Session,
    decoder: Session,
    vocos: Session,
    frontend: Frontend,
}
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ReferencePrompt {
    pub features: Vec<f32>,
    pub tokens: Vec<i64>,
    pub rms: f32,
    pub transcript: String,
}
impl Model {
    pub fn load(directory: &Path) -> anyhow::Result<Self> {
        let session = |name| {
            Session::builder()?
                .with_intra_threads(4)?
                .commit_from_file(directory.join(name))
        };
        Ok(Self {
            encoder: session("encoder.onnx")?,
            decoder: session("decoder.onnx")?,
            vocos: session("vocos.onnx")?,
            frontend: Frontend::load(directory)?,
        })
    }
    pub fn generate(
        &mut self,
        text: &str,
        reference: &[f32],
        transcript: &str,
        cancelled: impl Fn() -> bool,
    ) -> anyhow::Result<Option<Vec<f32>>> {
        let prompt = self.reference_prompt(reference, transcript)?;
        self.generate_with_prompt(text, &prompt, cancelled)
    }
    pub fn reference_prompt(
        &self,
        reference: &[f32],
        transcript: &str,
    ) -> anyhow::Result<ReferencePrompt> {
        let tokens = self.frontend.token_ids(transcript)?;
        let rms = (reference.iter().map(|v| v * v).sum::<f32>() / reference.len() as f32).sqrt();
        anyhow::ensure!(rms.is_finite() && rms > 1e-5, "silent reference");
        let normalized: Vec<_> = reference
            .iter()
            .map(|v| if rms < 0.1 { v * 0.1 / rms } else { *v })
            .collect();
        Ok(ReferencePrompt {
            features: audio::features(&normalized)?,
            tokens,
            rms,
            transcript: transcript.into(),
        })
    }
    pub fn generate_with_prompt(
        &mut self,
        text: &str,
        prompt: &ReferencePrompt,
        cancelled: impl Fn() -> bool,
    ) -> anyhow::Result<Option<Vec<f32>>> {
        anyhow::ensure!(
            !prompt.tokens.is_empty()
                && prompt.tokens.iter().all(|v| (0..360).contains(v))
                && !prompt.features.is_empty()
                && prompt.features.len().is_multiple_of(100)
                && prompt.features.len() <= (30 * 24000 + 128) / 256 * 100
                && prompt.features.iter().all(|v| v.is_finite())
                && prompt.rms.is_finite()
                && prompt.rms > 1e-5,
            "invalid Zip reference cache"
        );
        if cancelled() {
            return Ok(None);
        }
        let tokens = self.frontend.token_ids(text)?;
        let rms = prompt.rms;
        let features = &prompt.features;
        let reference_frames = features.len() / audio::MELS;
        let outputs = self.encoder.run(ort::inputs![
            "tokens" => Tensor::from_array(([1, tokens.len()], tokens))?,
            "prompt_tokens" => Tensor::from_array(([1, prompt.tokens.len()], prompt.tokens.clone()))?,
            "prompt_features_len" => Tensor::from_array((Vec::<usize>::new(), vec![reference_frames as i64]))?,
            "speed" => Tensor::from_array((Vec::<usize>::new(), vec![1.0_f32]))?
        ])?;
        let (shape, values) = outputs[0].try_extract_tensor::<f32>()?;
        anyhow::ensure!(
            shape.len() == 3 && shape[0] == 1 && shape[2] == 100,
            "invalid Zip encoder shape {shape:?}"
        );
        let frames = shape[1] as usize;
        anyhow::ensure!(
            frames > reference_frames && frames <= 10000,
            "Zip predicted invalid duration"
        );
        let condition = values.to_vec();
        drop(outputs);
        let mut speech = vec![0.0_f32; frames * 100];
        speech[..features.len()].copy_from_slice(features);
        let mut random = rand::rngs::StdRng::seed_from_u64(42);
        let mut x: Vec<f32> = (0..frames * 100)
            .map(|_| StandardNormal.sample(&mut random))
            .collect();
        let time = |step: usize| {
            let t = step as f32 / 8.0;
            0.5 * t / (1.0 - 0.5 * t)
        };
        for step in 0..8 {
            if cancelled() {
                return Ok(None);
            }
            let outputs = self.decoder.run(ort::inputs![
                "t" => Tensor::from_array((Vec::<usize>::new(), vec![time(step)]))?,
                "x" => Tensor::from_array(([1, frames, 100], x.clone()))?,
                "text_condition" => Tensor::from_array(([1, frames, 100], condition.clone()))?,
                "speech_condition" => Tensor::from_array(([1, frames, 100], speech.clone()))?,
                "guidance_scale" => Tensor::from_array((Vec::<usize>::new(), vec![3.0_f32]))?
            ])?;
            let (_, velocity) = outputs[0].try_extract_tensor::<f32>()?;
            anyhow::ensure!(velocity.len() == x.len(), "invalid flow output");
            let dt = time(step + 1) - time(step);
            for (value, velocity) in x.iter_mut().zip(velocity) {
                *value += velocity * dt;
            }
        }
        if cancelled() {
            return Ok(None);
        }
        let predicted = frames - reference_frames;
        let mel: Vec<_> = (0..100)
            .flat_map(|bin| (reference_frames..frames).map(move |frame| (bin, frame)))
            .map(|(bin, frame)| x[frame * 100 + bin] * 10.0)
            .collect();
        let outputs = self
            .vocos
            .run(ort::inputs!["mels" => Tensor::from_array(([1,100,predicted],mel))?])?;
        let (_, magnitude) = outputs["mag"].try_extract_tensor::<f32>()?;
        let (_, real) = outputs["x"].try_extract_tensor::<f32>()?;
        let (_, imaginary) = outputs["y"].try_extract_tensor::<f32>()?;
        if cancelled() {
            return Ok(None);
        }
        let mut samples = audio::waveform(magnitude, real, imaginary, predicted)?;
        for sample in &mut samples {
            *sample = sample.clamp(-1.0, 1.0) * if rms < 0.1 { rms / 0.1 } else { 1.0 };
        }
        anyhow::ensure!(
            !samples.is_empty() && samples.iter().all(|v| v.is_finite()),
            "invalid Zip PCM"
        );
        Ok(Some(samples))
    }
}
