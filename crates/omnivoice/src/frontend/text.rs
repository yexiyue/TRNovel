use std::{collections::HashSet, sync::OnceLock};

use regex::Regex;
use tokenizers::Tokenizer;

use crate::error::Result;

static NEWLINE_REGEX: OnceLock<Regex> = OnceLock::new();
static SPACE_REGEX: OnceLock<Regex> = OnceLock::new();
static NONVERBAL_TAG_REGEX: OnceLock<Regex> = OnceLock::new();

pub fn add_punctuation(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        return text.to_string();
    }

    let end_punctuation = HashSet::from([
        ';', ':', ',', '.', '!', '?', '…', ')', ']', '}', '"', '\'', '“', '”', '‘', '’', '；',
        '：', '，', '。', '！', '？', '、', '）', '】',
    ]);

    if end_punctuation.contains(&text.chars().last().unwrap_or_default()) {
        text.to_string()
    } else if text.chars().any(is_cjk) {
        format!("{text}。")
    } else {
        format!("{text}.")
    }
}

pub fn combine_text(text: &str, ref_text: Option<&str>) -> String {
    let mut full_text = if let Some(ref_text) = ref_text.filter(|value| !value.is_empty()) {
        format!("{} {}", ref_text.trim(), text.trim())
    } else {
        text.trim().to_string()
    };

    full_text = newline_regex().replace_all(&full_text, "").into_owned();
    full_text = full_text.replace('\u{ff08}', "(").replace('\u{ff09}', ")");
    full_text = space_regex().replace_all(&full_text, " ").into_owned();
    full_text = remove_spaces_around_cjk(&full_text);
    full_text
}

pub fn tokenize_with_nonverbal_tags(tokenizer: &Tokenizer, text: &str) -> Result<Vec<u32>> {
    let mut ids = Vec::new();
    let mut last_end = 0;
    let mut found_tag = false;

    for matched in nonverbal_tag_regex().find_iter(text) {
        found_tag = true;
        if matched.start() > last_end {
            ids.extend(
                tokenizer
                    .encode(&text[last_end..matched.start()], false)?
                    .get_ids()
                    .iter()
                    .copied(),
            );
        }
        ids.extend(
            tokenizer
                .encode(matched.as_str(), false)?
                .get_ids()
                .iter()
                .copied(),
        );
        last_end = matched.end();
    }

    if !found_tag {
        return Ok(tokenizer.encode(text, false)?.get_ids().to_vec());
    }

    if last_end < text.len() {
        ids.extend(
            tokenizer
                .encode(&text[last_end..], false)?
                .get_ids()
                .iter()
                .copied(),
        );
    }
    Ok(ids)
}

pub fn chunk_text_punctuation(
    text: &str,
    chunk_len: usize,
    min_chunk_len: Option<usize>,
) -> Vec<String> {
    let abbreviations: HashSet<&'static str> = HashSet::from([
        "Mr.", "Mrs.", "Ms.", "Dr.", "Prof.", "Sr.", "Jr.", "Rev.", "Fr.", "Hon.", "Pres.", "Gov.",
        "Capt.", "Gen.", "Sen.", "Rep.", "Col.", "Maj.", "Lt.", "Cmdr.", "Sgt.", "Cpl.", "Co.",
        "Corp.", "Inc.", "Ltd.", "Est.", "Dept.", "St.", "Ave.", "Blvd.", "Rd.", "Mt.", "Ft.",
        "No.", "Jan.", "Feb.", "Mar.", "Apr.", "Aug.", "Sep.", "Sept.", "Oct.", "Nov.", "Dec.",
        "i.e.", "e.g.", "vs.", "Vs.", "Etc.", "approx.", "fig.", "def.",
    ]);
    let split_punctuation: HashSet<char> = ".,;:!?。，；：！？".chars().collect();
    let closing_marks: HashSet<char> = "\"'“”‘’）]》>」】".chars().collect();

    let mut sentences: Vec<Vec<char>> = Vec::new();
    let mut current = Vec::new();
    for token in text.chars() {
        if current.is_empty()
            && !sentences.is_empty()
            && (split_punctuation.contains(&token) || closing_marks.contains(&token))
        {
            if let Some(last) = sentences.last_mut() {
                last.push(token);
            }
            continue;
        }
        current.push(token);
        if split_punctuation.contains(&token) {
            let is_abbreviation = if token == '.' {
                let current_str: String = current.iter().collect();
                current_str
                    .split_whitespace()
                    .last()
                    .is_some_and(|word| abbreviations.contains(word))
            } else {
                false
            };
            if !is_abbreviation {
                sentences.push(std::mem::take(&mut current));
            }
        }
    }
    if !current.is_empty() {
        sentences.push(current);
    }

    let mut merged: Vec<Vec<char>> = Vec::new();
    let mut chunk = Vec::new();
    for sentence in sentences {
        if chunk.len() + sentence.len() <= chunk_len {
            chunk.extend(sentence);
        } else {
            if !chunk.is_empty() {
                merged.push(std::mem::take(&mut chunk));
            }
            chunk = sentence;
        }
    }
    if !chunk.is_empty() {
        merged.push(chunk);
    }

    let mut final_chunks: Vec<Vec<char>> = Vec::new();
    if let Some(min_len) = min_chunk_len {
        let first_short = merged.first().is_some_and(|first| first.len() < min_len);
        for (index, item) in merged.into_iter().enumerate() {
            if index == 1 && first_short {
                if let Some(first) = final_chunks.last_mut() {
                    first.extend(item);
                }
                continue;
            }
            if item.len() >= min_len || final_chunks.is_empty() {
                final_chunks.push(item);
            } else if let Some(last) = final_chunks.last_mut() {
                last.extend(item);
            }
        }
    } else {
        final_chunks = merged;
    }

    final_chunks
        .into_iter()
        .map(|chunk| chunk.into_iter().collect::<String>().trim().to_string())
        .filter(|chunk| !chunk.is_empty())
        .collect()
}

fn is_cjk(ch: char) -> bool {
    ('\u{4e00}'..='\u{9fff}').contains(&ch)
}

fn newline_regex() -> &'static Regex {
    NEWLINE_REGEX.get_or_init(|| Regex::new(r"[\r\n]+").expect("valid newline regex"))
}

fn space_regex() -> &'static Regex {
    SPACE_REGEX.get_or_init(|| Regex::new(r"[ \t]+").expect("valid space regex"))
}

fn nonverbal_tag_regex() -> &'static Regex {
    NONVERBAL_TAG_REGEX.get_or_init(|| {
        Regex::new(
            r"\[(laughter|sigh|confirmation-en|question-en|question-ah|question-oh|question-ei|question-yi|surprise-ah|surprise-oh|surprise-wa|surprise-yo|dissatisfaction-hnn)\]",
        )
        .expect("valid non-verbal tag regex")
    })
}

fn remove_spaces_around_cjk(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut compact = String::with_capacity(chars.len());
    let mut index = 0;
    while index < chars.len() {
        if !chars[index].is_whitespace() {
            compact.push(chars[index]);
            index += 1;
            continue;
        }

        let start = index;
        while index < chars.len() && chars[index].is_whitespace() {
            index += 1;
        }
        let prev = start.checked_sub(1).and_then(|i| chars.get(i)).copied();
        let next = chars.get(index).copied();
        if !prev.is_some_and(is_cjk) && !next.is_some_and(is_cjk) {
            compact.extend(chars[start..index].iter().copied());
        }
    }
    compact
}

#[cfg(test)]
mod tests {
    use super::combine_text;

    #[test]
    fn combine_text_normalizes_python_parentheses_rules() {
        assert_eq!(combine_text("（你好）\nworld", None), "(你好)world");
    }

    #[test]
    fn combine_text_matches_python_empty_reference_and_cjk_whitespace_rules() {
        assert_eq!(combine_text("hello", Some("")), "hello");
        assert_eq!(combine_text("中   文", None), "中文");
    }
}
