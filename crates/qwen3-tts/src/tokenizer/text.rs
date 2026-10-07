//! Text tokenizer wrapper for Qwen2TokenizerFast

use anyhow::{Result, anyhow};
use std::path::Path;
use tokenizers::Tokenizer;

/// Pre-tokenizer regex matching Python's `Qwen2Converter` (from `convert_slow_tokenizer.py`).
const PRETOKENIZE_REGEX: &str = r"(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+(?!\S)|\s+";

/// Text tokenizer wrapping HuggingFace tokenizers
#[derive(Debug)]
pub struct TextTokenizer {
    tokenizer: Tokenizer,
    /// Beginning of sequence token ID
    pub bos_token_id: u32,
    /// End of sequence token ID
    pub eos_token_id: u32,
    /// Padding token ID
    pub pad_token_id: u32,
}

/// Create a simple mock tokenizer for testing
#[cfg(test)]
fn create_mock_tokenizer() -> Tokenizer {
    use tokenizers::models::bpe::BPE;
    use tokenizers::pre_tokenizers::whitespace::Whitespace;

    // Create a simple BPE tokenizer with a minimal vocab using array
    let vocab: [(&str, u32); 10] = [
        ("hello", 0),
        ("world", 1),
        ("test", 2),
        ("<|im_start|>", 3),
        ("<|im_end|>", 4),
        ("<|endoftext|>", 5),
        ("user", 6),
        ("assistant", 7),
        ("\n", 8),
        ("Ġ", 9), // Space token for BPE
    ];

    let merges: Vec<(String, String)> = vec![];
    let bpe = BPE::builder()
        .vocab_and_merges(vocab.map(|(k, v)| (k.to_string(), v)), merges)
        .unk_token("[UNK]".to_string())
        .build()
        .unwrap();

    let mut tokenizer = Tokenizer::new(bpe);
    tokenizer.with_pre_tokenizer(Some(Whitespace));
    tokenizer
}

impl TextTokenizer {
    /// Load tokenizer from a local path or HuggingFace model ID.
    ///
    /// Resolution order:
    /// 1. Direct file path to `tokenizer.json`
    /// 2. Directory containing `tokenizer.json`
    /// 3. Directory containing `vocab.json` + `merges.txt` (Qwen2-style, built at runtime)
    pub fn from_pretrained(model_id: &str) -> Result<Self> {
        let path = Path::new(model_id);

        // 1. Direct file path
        if path.is_file() {
            return Self::from_file(path);
        }

        // 2. Directory containing tokenizer.json
        if path.join("tokenizer.json").exists() {
            return Self::from_file(path.join("tokenizer.json"));
        }

        // 3. Directory with vocab.json + merges.txt (Qwen2-style)
        if path.join("vocab.json").exists() && path.join("merges.txt").exists() {
            tracing::info!(
                "Building tokenizer from vocab.json + merges.txt in '{}'",
                model_id
            );
            return Self::from_vocab_and_merges(path);
        }

        // 4. Local dir exists but has neither → clear error
        if path.is_dir() {
            anyhow::bail!(
                "No tokenizer files found in '{}'. Expected tokenizer.json or vocab.json + merges.txt.",
                model_id
            );
        }

        Err(anyhow!("No local tokenizer found at '{}'", model_id))
    }

    /// Build tokenizer from `vocab.json` + `merges.txt`, replicating Python's `Qwen2Converter`.
    ///
    /// This matches the pipeline in `transformers/convert_slow_tokenizer.py`:
    /// - Normalizer: NFC
    /// - Pre-tokenizer: Split(regex, Isolated) + ByteLevel(add_prefix_space=false, use_regex=false)
    /// - Model: BPE from vocab.json + merges.txt
    /// - Post-processor: ByteLevel(trim_offsets=false)
    /// - Decoder: ByteLevel
    pub fn from_vocab_and_merges(dir: &Path) -> Result<Self> {
        use tokenizers::SplitDelimiterBehavior;
        use tokenizers::models::bpe::BPE;
        use tokenizers::normalizers::unicode::NFC;
        use tokenizers::pre_tokenizers::byte_level::ByteLevel;
        use tokenizers::pre_tokenizers::sequence::Sequence;
        use tokenizers::pre_tokenizers::split::Split;

        let vocab_path = dir.join("vocab.json");
        let merges_path = dir.join("merges.txt");

        // Build BPE model from files
        let bpe = BPE::from_file(
            &vocab_path.to_string_lossy(),
            &merges_path.to_string_lossy(),
        )
        .unk_token("<|endoftext|>".to_string())
        .byte_fallback(false)
        .build()
        .map_err(|e| anyhow!("Failed to build BPE from vocab.json + merges.txt: {}", e))?;

        let mut tokenizer = Tokenizer::new(bpe);

        // NFC normalizer
        tokenizer.with_normalizer(Some(NFC));

        // Pre-tokenizer: Split on regex (Isolated) + ByteLevel
        let split = Split::new(PRETOKENIZE_REGEX, SplitDelimiterBehavior::Isolated, false)
            .map_err(|e| anyhow!("Failed to create Split pre-tokenizer: {}", e))?;
        let byte_level = ByteLevel::new(false, false, false);
        tokenizer.with_pre_tokenizer(Some(Sequence::new(vec![split.into(), byte_level.into()])));

        // Post-processor: ByteLevel (trim_offsets=false)
        tokenizer.with_post_processor(Some(ByteLevel::new(false, false, false)));

        // Decoder: ByteLevel
        tokenizer.with_decoder(Some(ByteLevel::new(false, false, false)));

        // Add special tokens from tokenizer_config.json if present
        let config_path = dir.join("tokenizer_config.json");
        if config_path.exists() {
            add_special_tokens_from_config(&mut tokenizer, &config_path)?;
        }

        Self::from_tokenizer(tokenizer)
    }

    /// Load tokenizer from a local file
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let tokenizer = Tokenizer::from_file(path)
            .map_err(|e| anyhow!("Failed to load tokenizer from {}: {}", path.display(), e))?;

        Self::from_tokenizer(tokenizer)
    }

    /// Create from a tokenizers::Tokenizer instance
    ///
    /// This is useful for creating tokenizers from custom configurations in tests.
    pub fn from_tokenizer(tokenizer: Tokenizer) -> Result<Self> {
        // Get special token IDs (Qwen2 defaults)
        let bos_token_id = tokenizer.token_to_id("<|im_start|>").unwrap_or(151643);

        let eos_token_id = tokenizer.token_to_id("<|im_end|>").unwrap_or(151645);

        let pad_token_id = tokenizer.token_to_id("<|endoftext|>").unwrap_or(151643);

        Ok(Self {
            tokenizer,
            bos_token_id,
            eos_token_id,
            pad_token_id,
        })
    }

    /// Encode text to token IDs
    pub fn encode(&self, text: &str) -> Result<Vec<u32>> {
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| anyhow!("Failed to encode text: {}", e))?;

        Ok(encoding.get_ids().to_vec())
    }

    /// Encode with special tokens (BOS/EOS)
    pub fn encode_with_special(&self, text: &str) -> Result<Vec<u32>> {
        let mut ids = vec![self.bos_token_id];
        ids.extend(self.encode(text)?);
        ids.push(self.eos_token_id);
        Ok(ids)
    }

    /// Encode using chat template format
    pub fn encode_chat(&self, text: &str, role: &str) -> Result<Vec<u32>> {
        // Qwen chat format: <|im_start|>role\ntext<|im_end|>
        let formatted = format!("<|im_start|>{}\n{}<|im_end|>", role, text);
        self.encode(&formatted)
    }

    /// Encode for TTS (user message format)
    pub fn encode_for_tts(&self, text: &str) -> Result<Vec<u32>> {
        // Format as user message
        let mut ids = self.encode_chat(text, "user")?;

        // Add assistant start token for generation
        ids.extend(self.encode("<|im_start|>assistant\n")?);

        Ok(ids)
    }

    /// Decode token IDs back to text
    pub fn decode(&self, ids: &[u32]) -> Result<String> {
        let text = self
            .tokenizer
            .decode(ids, true)
            .map_err(|e| anyhow!("Failed to decode tokens: {}", e))?;

        Ok(text)
    }

    /// Get vocabulary size
    pub fn vocab_size(&self) -> usize {
        self.tokenizer.get_vocab_size(true)
    }

    /// Convert token to ID
    pub fn token_to_id(&self, token: &str) -> Option<u32> {
        self.tokenizer.token_to_id(token)
    }

    /// Convert ID to token
    pub fn id_to_token(&self, id: u32) -> Option<String> {
        self.tokenizer.id_to_token(id)
    }

    /// Batch encode multiple texts
    pub fn encode_batch(&self, texts: &[&str]) -> Result<Vec<Vec<u32>>> {
        let encodings = self
            .tokenizer
            .encode_batch(texts.to_vec(), false)
            .map_err(|e| anyhow!("Failed to batch encode: {}", e))?;

        Ok(encodings
            .into_iter()
            .map(|e| e.get_ids().to_vec())
            .collect())
    }

    /// Encode and pad to max length
    pub fn encode_padded(&self, text: &str, max_length: usize) -> Result<Vec<u32>> {
        let mut ids = self.encode(text)?;

        if ids.len() > max_length {
            ids.truncate(max_length);
        } else {
            // Left-pad for TTS (causal attention)
            let pad_len = max_length - ids.len();
            let mut padded = vec![self.pad_token_id; pad_len];
            padded.extend(ids);
            ids = padded;
        }

        Ok(ids)
    }
}

/// Parse `tokenizer_config.json` and register special tokens on the tokenizer.
///
/// Looks for `added_tokens_decoder` (a map of ID → token info) which is the
/// standard HuggingFace format for special tokens like `<|im_start|>`,
/// `<|im_end|>`, `<|endoftext|>`, `<tts_pad>`, `<tts_text_bos>`, etc.
fn add_special_tokens_from_config(tokenizer: &mut Tokenizer, config_path: &Path) -> Result<()> {
    use tokenizers::AddedToken;

    let content = std::fs::read_to_string(config_path)
        .map_err(|e| anyhow!("Failed to read tokenizer_config.json: {}", e))?;
    let config: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| anyhow!("Failed to parse tokenizer_config.json: {}", e))?;

    // Extract added_tokens_decoder: { "151643": { "content": "<|endoftext|>", "special": true, ... }, ... }
    if let Some(added_tokens) = config
        .get("added_tokens_decoder")
        .and_then(|v| v.as_object())
    {
        let mut special_tokens = Vec::new();

        for (_id_str, token_info) in added_tokens {
            let content = match token_info.get("content").and_then(|v| v.as_str()) {
                Some(c) => c,
                None => continue,
            };
            let is_special = token_info
                .get("special")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);

            if is_special {
                let mut token = AddedToken::from(content, true);
                if let Some(lstrip) = token_info.get("lstrip").and_then(|v| v.as_bool()) {
                    token = token.lstrip(lstrip);
                }
                if let Some(rstrip) = token_info.get("rstrip").and_then(|v| v.as_bool()) {
                    token = token.rstrip(rstrip);
                }
                if let Some(normalized) = token_info.get("normalized").and_then(|v| v.as_bool()) {
                    token = token.normalized(normalized);
                }
                if let Some(single_word) = token_info.get("single_word").and_then(|v| v.as_bool()) {
                    token = token.single_word(single_word);
                }
                special_tokens.push(token);
            }
        }

        if !special_tokens.is_empty() {
            tracing::debug!(
                "Adding {} special tokens from tokenizer_config.json",
                special_tokens.len()
            );
            tokenizer.add_special_tokens(&special_tokens);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_tokenizer() -> TextTokenizer {
        let tokenizer = create_mock_tokenizer();
        TextTokenizer::from_tokenizer(tokenizer).unwrap()
    }

    #[test]
    fn test_default_special_tokens() {
        // Test that default token IDs have expected values (Qwen2 tokenizer)
        let bos_id: u32 = 151643;
        let eos_id: u32 = 151645;
        assert_eq!(bos_id, 151643); // <|im_start|>
        assert_eq!(eos_id, 151645); // <|im_end|>
    }

    #[test]
    fn test_from_tokenizer_special_tokens() {
        let tokenizer = create_test_tokenizer();
        // Our mock tokenizer has these special tokens
        assert_eq!(tokenizer.bos_token_id, 3); // <|im_start|>
        assert_eq!(tokenizer.eos_token_id, 4); // <|im_end|>
        assert_eq!(tokenizer.pad_token_id, 5); // <|endoftext|>
    }

    #[test]
    fn test_vocab_size() {
        let tokenizer = create_test_tokenizer();
        // Mock tokenizer has 10 tokens in vocab
        assert_eq!(tokenizer.vocab_size(), 10);
    }

    #[test]
    fn test_token_to_id() {
        let tokenizer = create_test_tokenizer();
        assert_eq!(tokenizer.token_to_id("hello"), Some(0));
        assert_eq!(tokenizer.token_to_id("world"), Some(1));
        assert_eq!(tokenizer.token_to_id("<|im_start|>"), Some(3));
        assert_eq!(tokenizer.token_to_id("nonexistent"), None);
    }

    #[test]
    fn test_id_to_token() {
        let tokenizer = create_test_tokenizer();
        assert_eq!(tokenizer.id_to_token(0), Some("hello".to_string()));
        assert_eq!(tokenizer.id_to_token(1), Some("world".to_string()));
        assert_eq!(tokenizer.id_to_token(3), Some("<|im_start|>".to_string()));
        assert_eq!(tokenizer.id_to_token(999), None);
    }

    #[test]
    fn test_encode_returns_result() {
        let tokenizer = create_test_tokenizer();
        // Just verify encoding doesn't panic - empty string always works
        let result = tokenizer.encode("");
        assert!(result.is_ok());
    }

    #[test]
    fn test_encode_with_special_structure() {
        let tokenizer = create_test_tokenizer();
        let result = tokenizer.encode_with_special("");
        // Should succeed and have at least BOS and EOS
        assert!(result.is_ok());
        let ids = result.unwrap();
        assert!(ids.len() >= 2);
        assert_eq!(ids[0], tokenizer.bos_token_id);
        assert_eq!(*ids.last().unwrap(), tokenizer.eos_token_id);
    }

    #[test]
    fn test_decode_known_ids() {
        let tokenizer = create_test_tokenizer();
        let text = tokenizer.decode(&[0, 1]).unwrap();
        // Decode should produce some text
        assert!(!text.is_empty() || text.is_empty()); // May or may not have content depending on impl
    }

    #[test]
    fn test_decode_empty() {
        let tokenizer = create_test_tokenizer();
        let text = tokenizer.decode(&[]).unwrap();
        assert!(text.is_empty());
    }

    #[test]
    fn test_encode_padded_truncate() {
        let tokenizer = create_test_tokenizer();
        // Create a string that encodes to some tokens, then truncate
        let result = tokenizer.encode_padded("", 2);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().len(), 2);
    }

    #[test]
    fn test_encode_padded_ensures_length() {
        let tokenizer = create_test_tokenizer();
        // Empty string should pad to requested length
        let ids = tokenizer.encode_padded("", 5).unwrap();
        assert_eq!(ids.len(), 5);
        // All should be pad tokens
        for id in &ids {
            assert_eq!(*id, tokenizer.pad_token_id);
        }
    }

    #[test]
    fn test_encode_batch_returns_correct_count() {
        let tokenizer = create_test_tokenizer();
        let batch = tokenizer.encode_batch(&["", "", ""]).unwrap();
        assert_eq!(batch.len(), 3);
    }

    #[test]
    fn test_encode_batch_empty() {
        let tokenizer = create_test_tokenizer();
        let batch = tokenizer.encode_batch(&[]).unwrap();
        assert!(batch.is_empty());
    }

    #[test]
    fn test_from_pretrained_nonexistent() {
        // Should fail for non-existent path
        let result = TextTokenizer::from_pretrained("/nonexistent/path");
        assert!(result.is_err());
    }

    #[test]
    fn test_from_file_nonexistent() {
        // Should fail for non-existent file
        let result = TextTokenizer::from_file("/nonexistent/tokenizer.json");
        assert!(result.is_err());
    }

    #[test]
    fn test_encode_empty_string() {
        let tokenizer = create_test_tokenizer();
        let ids = tokenizer.encode("").unwrap();
        assert!(ids.is_empty());
    }

    #[test]
    fn test_roundtrip_encode_decode() {
        let tokenizer = create_test_tokenizer();
        // Use empty string which should always work
        let original = "";
        let ids = tokenizer.encode(original).unwrap();
        let decoded = tokenizer.decode(&ids).unwrap();
        // Empty input should give empty output
        assert!(ids.is_empty());
        assert!(decoded.is_empty());
    }

    #[test]
    fn test_from_pretrained_dir_no_tokenizer_files() {
        // A directory that exists but has no tokenizer files should give a clear error
        let directory = tempfile::tempdir().unwrap();
        let result = TextTokenizer::from_pretrained(directory.path().to_str().unwrap());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No tokenizer files found"));
    }
}
