//! Text/audio alignment stays independent of model runtimes.
use crate::backend::Pcm;
use std::{future::Future, pin::Pin, sync::Arc};
use tts_protocol::TextRange;

#[derive(Clone, Debug)]
pub struct SpeechUnit {
    pub text: String,
    pub range: TextRange,
}
#[derive(Clone, Debug)]
pub struct Sentence {
    pub range: TextRange,
    pub units: std::ops::Range<usize>,
}
#[derive(Clone, Debug)]
pub struct SpeechText {
    pub units: Vec<SpeechUnit>,
    pub sentences: Vec<Sentence>,
}
#[derive(Clone, Debug)]
pub struct AudioClip {
    pub blocks: Vec<Arc<Pcm>>,
    pub sample_rate: u32,
    pub channels: u16,
    /// Keeps budget permits alive while an inference thread still owns the PCM.
    pub retention: Vec<Arc<dyn Send + Sync + std::fmt::Debug>>,
}
impl AudioClip {
    pub fn frames(&self) -> u64 {
        self.blocks
            .iter()
            .map(|block| block.samples.len() as u64 / u64::from(self.channels.max(1)))
            .sum()
    }
}
#[derive(Clone, Debug)]
pub struct SentenceTiming {
    pub range: TextRange,
    pub start_frame: u64,
    pub end_frame: u64,
}
pub type Alignment<'a> = Pin<Box<dyn Future<Output = Result<Vec<SentenceTiming>, String>> + 'a>>;
pub trait Aligner: Send + Sync {
    fn align<'a>(&'a self, text: &'a SpeechText, audio: &'a AudioClip) -> Alignment<'a>;
}
fn chinese(c: char) -> bool {
    matches!(c, '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' | '\u{20000}'..='\u{2ceaf}')
}
impl SpeechText {
    /// Match upstream alignment units while preserving exact source coordinates.
    pub fn from_source(text: &str, offset: usize) -> Self {
        let mut result = Self {
            units: Vec::new(),
            sentences: Vec::new(),
        };
        let mut word_start = None;
        let mut sentence_start = 0;
        let mut sentence_unit = 0;
        let mut line_offset = 0;
        let flush = |end: usize, start: &mut Option<usize>, units: &mut Vec<SpeechUnit>| {
            if let Some(start) = start.take() {
                // Upstream removes punctuation from space-delimited words.
                let cleaned: String = text[start..end]
                    .chars()
                    .filter(|c| c.is_alphanumeric() || *c == '\'')
                    .collect();
                if !cleaned.is_empty() {
                    units.push(SpeechUnit {
                        text: cleaned,
                        range: TextRange {
                            start: offset + start,
                            end: offset + end,
                        },
                    });
                }
            }
        };
        for line in text.split_inclusive('\n') {
            if crate::text::is_decoration_line(line) {
                line_offset += line.len();
                sentence_start = line_offset;
                continue;
            }
            for (i, c) in line.char_indices() {
                let start = line_offset + i;
                let end = start + c.len_utf8();
                let sentence_end = "。！？!?；;\n".contains(c)
                    || (c == '.'
                        && !text[end..]
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_digit()));
                if chinese(c) {
                    flush(start, &mut word_start, &mut result.units);
                    result.units.push(SpeechUnit {
                        text: c.to_string(),
                        range: TextRange {
                            start: offset + start,
                            end: offset + end,
                        },
                    });
                } else if c.is_alphanumeric() || c == '\'' {
                    word_start.get_or_insert(start);
                } else if c.is_whitespace() || sentence_end {
                    flush(start, &mut word_start, &mut result.units);
                }
                if sentence_end {
                    if result.units.len() > sentence_unit {
                        result.sentences.push(Sentence {
                            range: TextRange {
                                start: offset + sentence_start,
                                end: offset + end,
                            },
                            units: sentence_unit..result.units.len(),
                        });
                    }
                    sentence_unit = result.units.len();
                    sentence_start = end;
                }
            }
            line_offset += line.len();
        }
        flush(text.len(), &mut word_start, &mut result.units);
        if result.units.len() > sentence_unit {
            result.sentences.push(Sentence {
                range: TextRange {
                    start: offset + sentence_start,
                    end: offset + text.len(),
                },
                units: sentence_unit..result.units.len(),
            });
        }
        result
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decimals_and_hyphenated_words_keep_official_cleaned_units() {
        let speech = SpeechText::from_source("Rust 1.89 is fast-paced. Don't stop!", 0);
        assert_eq!(
            speech
                .units
                .iter()
                .map(|unit| unit.text.as_str())
                .collect::<Vec<_>>(),
            ["Rust", "189", "is", "fastpaced", "Don't", "stop"]
        );
        assert_eq!(speech.sentences.len(), 2);
        assert_eq!(speech.units[1].range, TextRange { start: 5, end: 9 });
    }
    #[test]
    fn source_mapping_retains_unicode_and_skips_decorations() {
        let text = "=====\r\n你好。Welcome reader!";
        let speech = SpeechText::from_source(text, 10);
        assert_eq!(
            speech
                .units
                .iter()
                .map(|unit| unit.text.as_str())
                .collect::<Vec<_>>(),
            vec!["你", "好", "Welcome", "reader"]
        );
        assert_eq!(speech.sentences.len(), 2);
        for unit in &speech.units {
            assert_eq!(&text[unit.range.start - 10..unit.range.end - 10], unit.text);
        }
    }
}
