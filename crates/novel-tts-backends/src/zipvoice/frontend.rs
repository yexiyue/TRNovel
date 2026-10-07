//! Emilia segmentation, complete pinned word pronunciations and upstream tone rules.
mod english;
pub mod normalize;
use jieba_rs::Jieba;
use regex::Regex;
use std::{
    collections::HashMap,
    io::Cursor,
    path::{Path, PathBuf},
};

pub struct Frontend {
    jieba: Jieba,
    words: HashMap<String, Vec<String>>,
    characters: HashMap<String, String>,
    syllables: HashMap<String, Vec<String>>,
    tokens: HashMap<String, i64>,
    helper: PathBuf,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Language {
    Chinese,
    English,
    Other,
}
impl Frontend {
    pub fn load(directory: &Path) -> anyhow::Result<Self> {
        let mut dictionary = Cursor::new(include_bytes!("frontend/assets/jieba.dict"));
        let jieba = Jieba::with_dict(&mut dictionary)?;
        let mut tokens = HashMap::new();
        for line in std::fs::read_to_string(directory.join("tokens.txt"))?.lines() {
            let (token, id) = line
                .rsplit_once('\t')
                .ok_or_else(|| anyhow::anyhow!("invalid Zip token dictionary"))?;
            tokens.insert(token.into(), id.parse()?);
        }
        Ok(Self {
            jieba,
            words: serde_json::from_str(include_str!("frontend/assets/words.json"))?,
            characters: serde_json::from_str(include_str!("frontend/assets/characters.json"))?,
            syllables: serde_json::from_str(include_str!("frontend/assets/syllables.json"))?,
            tokens,
            helper: english::prepare(&directory.join("phonemizer"))?,
        })
    }
    pub fn token_ids(&self, text: &str) -> anyhow::Result<Vec<i64>> {
        let phones = self.phonemes(text)?;
        let ids: Vec<_> = phones
            .iter()
            .filter_map(|phone| self.tokens.get(phone).copied())
            .collect();
        anyhow::ensure!(!ids.is_empty(), "Zip frontend produced no supported tokens");
        Ok(ids)
    }
    pub fn phonemes(&self, text: &str) -> anyhow::Result<Vec<String>> {
        let mut text = text.to_string();
        for (from, to) in [
            ("，", ","),
            ("。", "."),
            ("！", "!"),
            ("？", "?"),
            ("；", ";"),
            ("：", ":"),
            ("、", ","),
            ("‘", "'"),
            ("“", "\""),
            ("”", "\""),
            ("’", "'"),
            ("⋯", "…"),
            ("···", "…"),
            ("・・・", "…"),
            ("...", "…"),
        ] {
            text = text.replace(from, to);
        }
        let special = Regex::new(r"[<\[].*?[>\]]|.").expect("Emilia parts expression");
        let mut segments = Vec::new();
        let mut current = String::new();
        let mut language = Language::Other;
        for part in special.find_iter(&text).map(|m| m.as_str()) {
            let kind = if part.starts_with('<') && part.ends_with('>')
                || part
                    .chars()
                    .next()
                    .is_some_and(|c| ('\u{4e00}'..='\u{9fa5}').contains(&c))
            {
                Language::Chinese
            } else if part.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
                Language::English
            } else {
                Language::Other
            };
            if current.is_empty() || language == Language::Other {
                current.push_str(part);
                language = kind;
            } else if kind == language || kind == Language::Other {
                current.push_str(part);
            } else {
                segments.push((std::mem::take(&mut current), language));
                current.push_str(part);
                language = kind;
            }
        }
        if !current.is_empty() {
            segments.push((current, language));
        }
        let specials = Regex::new(r"[<\[].*?[>\]]").expect("special span expression");
        let mut output = Vec::new();
        for (segment, language) in segments {
            let mut at = 0;
            for item in specials.find_iter(&segment) {
                self.append(&segment[at..item.start()], language, &mut output)?;
                let value = item.as_str();
                if value.starts_with('<') && value.ends_with('>') {
                    let syllable = &value[1..value.len() - 1];
                    output.extend(
                        self.syllables
                            .get(syllable)
                            .ok_or_else(|| anyhow::anyhow!("unsupported explicit pinyin {value}"))?
                            .iter()
                            .cloned(),
                    );
                } else {
                    output.push(value.into());
                }
                at = item.end();
            }
            self.append(&segment[at..], language, &mut output)?;
        }
        Ok(output)
    }
    fn append(
        &self,
        text: &str,
        language: Language,
        output: &mut Vec<String>,
    ) -> anyhow::Result<()> {
        if text.is_empty() {
            return Ok(());
        }
        match language {
            Language::English => {
                output.extend(english::phonemes(&self.helper, &normalize::english(text))?)
            }
            Language::Other => {}
            Language::Chinese => {
                let normalized = normalize::chinese(text);
                for token in self.jieba.cut(&normalized, true) {
                    let word = token.word;
                    if let Some(phones) = self.words.get(word) {
                        output.extend(phones.iter().cloned());
                        continue;
                    }
                    // Unknown HMM words still use the full character dictionary and the
                    // exact upstream ToneSandhiMixin order, rather than a guessed reading.
                    let chars: Vec<_> = word.chars().collect();
                    let mut syllables: Vec<_> = chars
                        .iter()
                        .map(|c| {
                            self.characters
                                .get(&c.to_string())
                                .cloned()
                                .unwrap_or_else(|| c.to_string())
                        })
                        .collect();
                    tone_sandhi(&chars, &mut syllables);
                    for syllable in syllables {
                        if let Some(phones) = self.syllables.get(&syllable) {
                            output.extend(phones.iter().cloned());
                        } else {
                            output.push(syllable);
                        }
                    }
                }
            }
        }
        Ok(())
    }
}
fn change_tone(syllable: &mut String, tone: u8) {
    if syllable
        .as_bytes()
        .last()
        .is_some_and(|c| matches!(c, b'1'..=b'5'))
    {
        syllable.pop();
        syllable.push(tone as char);
    }
}
fn tone_sandhi(chars: &[char], syllables: &mut [String]) {
    let tail = syllables
        .iter()
        .rev()
        .take_while(|s| s.ends_with('3'))
        .count();
    if tail >= 2 {
        let mut left = tail - 1;
        for syllable in syllables.iter_mut() {
            if left > 0 && syllable.ends_with('3') {
                change_tone(syllable, b'2');
                left -= 1;
            }
        }
    }
    for index in 0..chars.len() {
        if chars[index] == '不' {
            let tone = if syllables.get(index + 1).is_some_and(|s| s.ends_with('4')) {
                b'2'
            } else {
                b'4'
            };
            change_tone(&mut syllables[index], tone);
        }
    }
    for index in 0..chars.len() {
        if chars[index] == '一' {
            if index + 1 == chars.len() {
                change_tone(&mut syllables[index], b'1');
            } else if !syllables[index + 1].ends_with('4') {
                change_tone(&mut syllables[index], b'4');
            } else if syllables[index].ends_with('4') {
                change_tone(&mut syllables[index], b'2');
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[derive(serde::Deserialize)]
    struct Golden {
        text: String,
        phones: Vec<String>,
    }
    #[test]
    fn matches_pinned_emilia_dates_polyphones_mixed_english_and_explicit_pinyin() {
        let temporary = tempfile::tempdir().unwrap();
        std::fs::write(
            temporary.path().join("tokens.txt"),
            include_bytes!("frontend/assets/tokens.txt"),
        )
        .unwrap();
        let frontend = Frontend::load(temporary.path()).unwrap();
        let cases: Vec<Golden> =
            serde_json::from_str(include_str!("frontend/golden.json")).unwrap();
        for case in cases {
            assert_eq!(
                frontend.phonemes(&case.text).unwrap(),
                case.phones,
                "{}",
                case.text
            );
        }
    }
}
