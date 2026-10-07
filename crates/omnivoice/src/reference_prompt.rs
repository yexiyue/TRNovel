use crate::{
    contracts::{I64Tensor2, VoiceClonePrompt},
    error::{OmniVoiceError, Result},
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReferencePromptOptions {
    pub sample_rate: u32,
    pub hop_length: usize,
    pub expected_codebooks: usize,
}

impl Default for ReferencePromptOptions {
    fn default() -> Self {
        Self {
            sample_rate: 24_000,
            hop_length: 960,
            expected_codebooks: 8,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReferencePromptBuilder {
    options: ReferencePromptOptions,
}

impl ReferencePromptBuilder {
    pub fn new(options: ReferencePromptOptions) -> Self {
        Self { options }
    }

    pub fn trim_to_hop_multiple(&self, waveform: &[f32]) -> Vec<f32> {
        if self.options.hop_length == 0 {
            return waveform.to_vec();
        }
        let remainder = waveform.len() % self.options.hop_length;
        if remainder == 0 {
            waveform.to_vec()
        } else {
            waveform[..waveform.len() - remainder].to_vec()
        }
    }

    pub fn prompt_from_tokens(
        &self,
        tokens: I64Tensor2,
        ref_text: impl Into<String>,
        ref_rms: Option<f32>,
    ) -> Result<VoiceClonePrompt> {
        if tokens.dims().0 != self.options.expected_codebooks {
            return Err(OmniVoiceError::InvalidTensorShape {
                name: "ref_audio_tokens".to_string(),
                expected: format!("({}, T)", self.options.expected_codebooks),
                actual: format!("{:?}", tokens.dims()),
            });
        }
        if tokens.dims().1 == 0 {
            return Err(OmniVoiceError::InvalidRequest(
                "ref_audio_tokens must contain at least one frame".to_string(),
            ));
        }
        Ok(VoiceClonePrompt {
            ref_audio_tokens: tokens,
            ref_text: ref_text.into(),
            ref_rms,
        })
    }
}
