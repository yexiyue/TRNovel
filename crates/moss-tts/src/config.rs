//! Official model configuration, including distinct global and depth dimensions.
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
pub struct TransformerConfig {
    pub hidden_size: usize,
    pub intermediate_size: usize,
    pub num_hidden_layers: usize,
    pub num_attention_heads: usize,
    pub num_key_value_heads: usize,
    pub head_dim: usize,
    pub rms_norm_eps: f64,
    pub rope_theta: f64,
    #[serde(default)]
    pub vocab_size: usize,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SpeechConfig {
    pub language_config: TransformerConfig,
    pub n_vq: usize,
    pub audio_vocab_size: usize,
    pub audio_pad_code: u32,
    pub audio_start_token_id: u32,
    pub audio_end_token_id: u32,
    pub audio_user_slot_token_id: u32,
    pub audio_assistant_gen_slot_token_id: u32,
    pub sampling_rate: u32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LocalConfig {
    #[serde(flatten)]
    pub speech: SpeechConfig,
    pub local_hidden_size: usize,
    pub local_ffn_hidden_size: usize,
    pub local_num_layers: usize,
    pub additional_mlp_ffn_hidden_size: usize,
}
impl std::ops::Deref for LocalConfig {
    type Target = SpeechConfig;
    fn deref(&self) -> &SpeechConfig {
        &self.speech
    }
}

impl LocalConfig {
    pub fn depth_config(&self) -> TransformerConfig {
        TransformerConfig {
            hidden_size: self.local_hidden_size,
            intermediate_size: self.local_ffn_hidden_size,
            num_hidden_layers: self.local_num_layers,
            ..self.language_config.clone()
        }
    }
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.local_hidden_size > 0 && self.local_num_layers > 0,
            "invalid depth transformer"
        );
        self.speech.validate()
    }
}
impl SpeechConfig {
    pub fn validate(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.n_vq > 0 && self.n_vq <= 32 && self.audio_pad_code == self.audio_vocab_size as u32,
            "unsupported MOSS codebooks"
        );
        anyhow::ensure!(self.sampling_rate == 24000, "unsupported MOSS sample rate");
        Ok(())
    }
}
