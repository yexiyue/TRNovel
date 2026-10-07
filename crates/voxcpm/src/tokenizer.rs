//! SentencePiece BPE and the official VoxCPM Chinese character expansion.
use crate::weights::Weights;
use candle_core::{Result, quantized::gguf_file::Value};
use std::collections::HashMap;

pub struct Tokenizer {
    pieces: Vec<String>,
    scores: Vec<f32>,
    ids: HashMap<String, u32>,
    specials: Vec<(String, u32)>,
    native: Option<tokenizers::Tokenizer>,
    expansions: HashMap<u32, Vec<u32>>,
}
impl Tokenizer {
    pub(crate) fn load(w: &Weights) -> Result<Self> {
        let pieces = w
            .value("tokenizer.ggml.tokens")?
            .to_vec()?
            .iter()
            .map(|v| v.to_string().cloned())
            .collect::<Result<Vec<_>>>()?;
        let scores = w
            .value("tokenizer.ggml.scores")?
            .to_vec()?
            .iter()
            .map(Value::to_f32)
            .collect::<Result<Vec<_>>>()?;
        let ids = pieces
            .iter()
            .enumerate()
            .map(|(i, s)| (s.clone(), i as u32))
            .collect();
        let types = w.value("tokenizer.ggml.token_type")?.to_vec()?;
        if pieces.len() != scores.len() || pieces.len() != types.len() {
            candle_core::bail!("inconsistent GGUF tokenizer arrays");
        }
        let mut specials = Vec::new();
        for (i, typ) in types.iter().enumerate() {
            if matches!(typ.to_i32()?, 3 | 4) {
                specials.push((pieces[i].clone(), i as u32));
            }
        }
        Ok(Self {
            pieces,
            scores,
            ids,
            specials,
            native: None,
            expansions: HashMap::new(),
        })
    }
    pub(crate) fn from_file(path: &std::path::Path) -> Result<Self> {
        let native = tokenizers::Tokenizer::from_file(path)
            .map_err(|e| candle_core::Error::Msg(e.to_string()))?;
        let ids = native.get_vocab(true);
        let mut pieces = vec![String::new(); ids.values().copied().max().unwrap_or(0) as usize + 1];
        for (piece, &id) in &ids {
            pieces[id as usize] = piece.clone();
        }
        let expansions = ids
            .iter()
            .filter_map(|(piece, &id)| {
                let bare = piece.replace('▁', "");
                if bare.chars().count() < 2
                    || !bare.chars().all(is_chinese)
                    || !ids.contains_key(&bare)
                {
                    return None;
                }
                let chars: Option<Vec<_>> = bare
                    .chars()
                    .map(|c| ids.get(&c.to_string()).copied().filter(|id| *id != 0))
                    .collect();
                chars.map(|chars| (id, chars))
            })
            .collect();
        Ok(Self {
            pieces,
            scores: vec![],
            ids,
            specials: vec![],
            native: Some(native),
            expansions,
        })
    }
    /// Tokenize without BOS, then split merged Chinese pieces as upstream does.
    pub fn encode(&self, text: &str) -> Result<Vec<u32>> {
        self.encode_with_cancel(text, &|| false)
    }
    pub(crate) fn encode_with_cancel(
        &self,
        text: &str,
        cancel: &impl Fn() -> bool,
    ) -> Result<Vec<u32>> {
        if let Some(native) = &self.native {
            if cancel() {
                candle_core::bail!("tokenization cancelled");
            }
            let encoded = native
                .encode(text, false)
                .map_err(|e| candle_core::Error::Msg(e.to_string()))?;
            let mut output = Vec::new();
            for &id in encoded.get_ids() {
                if cancel() {
                    candle_core::bail!("tokenization cancelled");
                }
                if let Some(chars) = self.expansions.get(&id) {
                    output.extend(chars);
                } else {
                    output.push(id);
                }
            }
            return Ok(output);
        }
        let mut rest = text;
        let mut output = Vec::new();
        while !rest.is_empty() {
            if cancel() {
                candle_core::bail!("tokenization cancelled");
            }
            let next = self
                .specials
                .iter()
                .filter_map(|(piece, id)| rest.find(piece).map(|position| (position, piece, *id)))
                .min_by(|a, b| a.0.cmp(&b.0).then_with(|| b.1.len().cmp(&a.1.len())));
            let Some((position, piece, id)) = next else {
                output.extend(self.encode_piece(rest, cancel)?);
                break;
            };
            output.extend(self.encode_piece(&rest[..position], cancel)?);
            output.push(id);
            rest = &rest[position + piece.len()..];
        }
        Ok(output)
    }
    fn encode_piece(&self, text: &str, cancel: &impl Fn() -> bool) -> Result<Vec<u32>> {
        if text.is_empty() {
            return Ok(vec![]);
        }
        let normalized = format!("▁{}", text.replace(' ', "▁"));
        let mut symbols: Vec<String> = normalized.chars().map(|c| c.to_string()).collect();
        loop {
            if cancel() {
                candle_core::bail!("tokenization cancelled");
            }
            let best = symbols
                .windows(2)
                .enumerate()
                .filter_map(|(i, p)| {
                    let merged = format!("{}{}", p[0], p[1]);
                    self.ids
                        .get(&merged)
                        .map(|id| (i, merged, self.scores[*id as usize]))
                })
                .max_by(|a, b| a.2.total_cmp(&b.2).then_with(|| b.0.cmp(&a.0)));
            let Some((i, merged, _)) = best else {
                break;
            };
            symbols[i] = merged;
            symbols.remove(i + 1);
        }
        let mut out = Vec::new();
        for s in symbols {
            let bare = s.replace('▁', "");
            if bare.chars().count() > 1
                && bare.chars().all(is_chinese)
                && self.ids.contains_key(&bare)
            {
                for c in bare.chars() {
                    out.push(*self.ids.get(&c.to_string()).ok_or_else(|| {
                        candle_core::Error::Msg(format!("missing Chinese token {c}"))
                    })?);
                }
            } else if let Some(id) = self.ids.get(&s) {
                out.push(*id);
            } else {
                for byte in s.as_bytes() {
                    out.push(*self.ids.get(&format!("<0x{byte:02X}>")).ok_or_else(|| {
                        candle_core::Error::Msg("missing byte fallback token".into())
                    })?);
                }
            }
        }
        Ok(out)
    }
    pub fn piece(&self, id: u32) -> Option<&str> {
        self.pieces.get(id as usize).map(String::as_str)
    }
}

fn is_chinese(c: char) -> bool {
    matches!(c as u32, 0x4e00..=0x9fff)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "requires original tokenizer.json"]
    fn original_tokenizer_matches_reference_ids() {
        let directory = std::path::PathBuf::from(std::env::var("VOXCPM_ORIGINAL_MODELS").unwrap());
        let tokenizer = Tokenizer::from_file(&directory.join("tokenizer.json")).unwrap();
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/original-tokenizer.json"))
                .unwrap();
        for fixture in fixtures.as_array().unwrap() {
            let expected: Vec<u32> = serde_json::from_value(fixture["ids"].clone()).unwrap();
            assert_eq!(
                tokenizer.encode(fixture["text"].as_str().unwrap()).unwrap(),
                expected
            );
        }
        assert!(tokenizer.encode_with_cancel("取消分词", &|| true).is_err());
    }
    #[test]
    #[ignore = "requires the existing GGUF tokenizer"]
    fn sentencepiece_ids_match_reference() {
        let directory = std::path::PathBuf::from(
            std::env::var("VOXCPM_TEST_MODELS").expect("set VOXCPM_TEST_MODELS"),
        );
        let w = Weights::open(
            &directory.join("VoxCPM2-BaseLM-Q8_0.gguf"),
            &candle_core::Device::Cpu,
            candle_core::DType::F32,
        )
        .unwrap();
        let tokenizer = Tokenizer::load(&w).unwrap();
        let fixtures: serde_json::Value =
            serde_json::from_str(include_str!("../tests/fixtures/tokenizer.json")).unwrap();
        for fixture in fixtures.as_array().unwrap() {
            let expected: Vec<u32> = serde_json::from_value(fixture["ids"].clone()).unwrap();
            assert_eq!(
                tokenizer.encode(fixture["text"].as_str().unwrap()).unwrap(),
                expected
            );
        }
        assert_eq!(tokenizer.encode("<|audio_start|>").unwrap(), vec![101]);
        assert!(tokenizer.encode_with_cancel("取消分词", &|| true).is_err());
    }
}
