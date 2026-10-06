use sentencepiece_rs::SentencePieceProcessor;
use tts_core::{
    backend::BackendError,
    text::{TextSegment, is_decoration_line},
};

fn is_cjk(c: char) -> bool {
    matches!(c, '\u{4e00}'..='\u{9fff}' | '\u{3400}'..='\u{4dbf}' |
        '\u{3040}'..='\u{30ff}' | '\u{ac00}'..='\u{d7af}')
}

fn contains_cjk(text: &str) -> bool {
    text.chars().any(is_cjk)
}

pub(super) fn normalize(text: &str) -> String {
    let mut spoken = String::new();
    let mut previous_cjk = false;
    for line in text.lines().filter(|line| !is_decoration_line(line)) {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            continue;
        }
        // Preserve English word boundaries, but do not insert Chinese layout pauses.
        let line_cjk = contains_cjk(&line);
        let latin_words = spoken
            .chars()
            .last()
            .is_some_and(|c| c.is_ascii_alphanumeric())
            && line
                .chars()
                .next()
                .is_some_and(|c| c.is_ascii_alphanumeric());
        if !spoken.is_empty() && ((!previous_cjk && !line_cjk) || latin_words) {
            spoken.push(' ');
        }
        previous_cjk = line_cjk;
        spoken.push_str(&line);
    }
    let mut text = spoken;
    if text.is_empty() {
        return text;
    }
    if !text
        .trim_end_matches(is_closer)
        .ends_with(['。', '！', '？', '.', '!', '?', ';', '；'])
    {
        text.push(if contains_cjk(&text) { '。' } else { '.' });
    }
    if !contains_cjk(&text) {
        if let Some(first) = text.chars().next()
            && first.is_ascii_lowercase()
        {
            text.replace_range(..1, &first.to_ascii_uppercase().to_string());
        }
        if text.split_whitespace().count() < 5 {
            text = format!("        {text}");
        }
    }
    text
}

pub(super) fn segments_with_estimate(
    text: &str,
    tokenizer: &SentencePieceProcessor,
    estimate: &tts_core::text::duration::DurationEstimator,
) -> Result<Vec<TextSegment>, BackendError> {
    split_estimate(
        text,
        |s| {
            tokenizer
                .encode_to_ids(&normalize(s))
                .map(|ids| ids.len())
                .map_err(|e| BackendError::Synthesis(e.to_string()))
        },
        estimate,
    )
}

#[cfg(test)]
fn split(
    text: &str,
    count: impl Fn(&str) -> Result<usize, BackendError>,
) -> Result<Vec<TextSegment>, BackendError> {
    split_estimate(text, count, &Default::default())
}
fn split_estimate(
    text: &str,
    count: impl Fn(&str) -> Result<usize, BackendError>,
    estimate: &tts_core::text::duration::DurationEstimator,
) -> Result<Vec<TextSegment>, BackendError> {
    split_limit(text, count, estimate, false)
}
pub(super) fn first_segment(
    text: &str,
    tokenizer: &SentencePieceProcessor,
    estimate: &tts_core::text::duration::DurationEstimator,
) -> Result<Option<TextSegment>, BackendError> {
    Ok(split_limit(
        text,
        |s| {
            tokenizer
                .encode_to_ids(&normalize(s))
                .map(|ids| ids.len())
                .map_err(|e| BackendError::Synthesis(e.to_string()))
        },
        estimate,
        true,
    )?
    .into_iter()
    .next())
}
fn is_closer(c: char) -> bool {
    "”’\"'）)]】》」』".contains(c)
}

/// Whitespace-only lines, decorations and titles delimit synthesis context.
fn hard_line(line: &str) -> bool {
    line.trim().is_empty() || is_decoration_line(line) || tts_core::text::is_heading_line(line)
}

pub(super) fn paragraph_end(segment: &str, remaining: &str) -> bool {
    if remaining.trim().is_empty() || tts_core::text::is_heading_line(segment.trim()) {
        return true;
    }
    let next_line = if segment.ends_with('\n') {
        remaining
    } else if let Some((tail, next)) = remaining.split_once('\n') {
        if !tail.trim().is_empty() {
            return false;
        }
        next
    } else {
        return false;
    };
    next_line.lines().next().is_some_and(hard_line)
}

fn context_end(text: &str, start: usize) -> usize {
    let mut offset = start;
    for (index, line) in text[start..].split_inclusive('\n').enumerate() {
        if index == 0 && tts_core::text::is_heading_line(line) {
            return offset + line.len();
        }
        if index > 0 && hard_line(line) {
            return offset;
        }
        offset += line.len();
    }
    text.len()
}

fn split_limit(
    text: &str,
    count: impl Fn(&str) -> Result<usize, BackendError>,
    estimate: &tts_core::text::duration::DurationEstimator,
    first: bool,
) -> Result<Vec<TextSegment>, BackendError> {
    const MAX_TOKENS: usize = 50;
    const MAX_CJK_CHARS: usize = 60;
    let mut result = Vec::new();
    let mut start = 0;
    while start < text.len() {
        if start == 0 || text.as_bytes()[start - 1] == b'\n' {
            let line_end = text[start..]
                .find('\n')
                .map_or(text.len(), |offset| start + offset + 1);
            if text[start..line_end].trim().is_empty() || is_decoration_line(&text[start..line_end])
            {
                start = line_end;
                continue;
            }
        }
        let limit = context_end(text, start);
        let mut fitting = start;
        let mut sentence_boundary = start;
        let mut clause_boundary = start;
        let mut cjk_chars = 0;
        let mut after_sentence = false;
        let mut target_reached = false;
        for (byte, c) in text[start..limit].char_indices() {
            if target_reached && !is_closer(c) {
                break;
            }
            let end = start + byte + c.len_utf8();
            cjk_chars += usize::from(is_cjk(c));
            if cjk_chars > MAX_CJK_CHARS
                || count(&text[start..end])? > MAX_TOKENS
                || estimate.seconds(&text[start..end]) > 12.0
            {
                break;
            }
            // Prefer a completed sentence within the target before consuming
            // the next sentence into the larger emergency ceiling.
            if estimate.seconds(&text[start..end]) > 8.0
                && sentence_boundary > start
                && !after_sentence
            {
                break;
            }
            fitting = end;
            let sentence_end = "。！？!?；;".contains(c)
                || (c == '.'
                    && !text[end..]
                        .chars()
                        .next()
                        .is_some_and(|next| next.is_ascii_digit()));
            if sentence_end || (after_sentence && is_closer(c)) {
                sentence_boundary = end;
                after_sentence = true;
                target_reached = estimate.seconds(&text[start..end]) >= 8.0;
            } else {
                after_sentence = c.is_whitespace() && after_sentence;
                if "，,:：".contains(c) {
                    clause_boundary = end;
                }
            }
        }
        if fitting == start {
            return Err(BackendError::Unsupported(
                "one character exceeds the MOSS token budget".into(),
            ));
        }
        let end = if fitting == limit {
            fitting
        } else if sentence_boundary > start {
            sentence_boundary
        } else if clause_boundary > start {
            clause_boundary
        } else {
            fitting
        };
        if !text[start..end].trim().is_empty() {
            result.push(TextSegment {
                text: text[start..end].into(),
                start,
                end,
            });
        }
        start = end;
        if first && !result.is_empty() {
            break;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn soft_lines_merge_but_blank_lines_titles_and_separators_do_not() {
        let source =
            "第一章 山路\n清晨的风\r\n吹过林间。\n\n第二段。\n=====\nChapter 2 Dawn\n最后一句。";
        let pieces = split(source, |_| Ok(1)).unwrap();
        assert_eq!(
            pieces.iter().map(|p| p.text.trim()).collect::<Vec<_>>(),
            [
                "第一章 山路",
                "清晨的风\r\n吹过林间。",
                "第二段。",
                "Chapter 2 Dawn",
                "最后一句。"
            ]
        );
        for piece in &pieces {
            assert_eq!(&source[piece.start..piece.end], piece.text);
        }
        assert!(!paragraph_end("清晨。\n", "风吹过林间。"));
        assert!(paragraph_end("清晨。\n", "\n第二段。"));
        assert!(paragraph_end("清晨。", "\n\n第二段。"));
        assert!(!paragraph_end("清晨。", "\n第二行。"));
        assert!(paragraph_end("清晨。\n", "=====\n第二段。"));
        assert!(paragraph_end("第一章 山路\n", "清晨。"));
        assert_eq!(normalize("清晨的风\n吹过林间。"), "清晨的风吹过林间。");
        assert_eq!(
            normalize("hello\nmy friend today"),
            "        Hello my friend today."
        );
        assert!(!tts_core::text::is_heading_line("第一回合开始了。"));
    }
    #[test]
    fn target_prefers_previous_complete_sentence_before_ceiling() {
        let source = format!(
            "{}。\n{}。\n{}。",
            "中".repeat(18),
            "文".repeat(13),
            "甲".repeat(10)
        );
        let pieces = split(&source, |_| Ok(1)).unwrap();
        assert_eq!(pieces.len(), 2);
        assert_eq!(
            pieces[0].text.trim(),
            format!("{}。\n{}。", "中".repeat(18), "文".repeat(13))
        );
    }

    #[test]
    fn closing_quotes_follow_sentence_and_initial_duration_is_bounded() {
        let source = format!("“{}。”\n接下来的句子。", "中".repeat(32));
        let pieces = split(&source, |_| Ok(1)).unwrap();
        assert!(pieces[0].text.ends_with("。”"));
        let estimate = tts_core::text::duration::DurationEstimator::default();
        let text = "中文".repeat(120);
        for piece in split(&text, |_| Ok(1)).unwrap() {
            assert!(estimate.seconds(&piece.text) <= 12.0);
        }
        assert_eq!(normalize("“你好！”"), "“你好！”");
        assert_eq!(
            normalize("hello,\nfriend today."),
            "        Hello, friend today."
        );
    }

    #[test]
    fn ranges_cover_original_unicode_and_budget() {
        let text = format!("开头。{}\r\n结尾！", "你好世界".repeat(90));
        let pieces = split(&text, |s| Ok(s.chars().count())).unwrap();
        assert_eq!(pieces.first().unwrap().start, 0);
        assert_eq!(pieces.last().unwrap().end, text.len());
        for piece in &pieces {
            assert_eq!(&text[piece.start..piece.end], piece.text);
            assert!(piece.text.chars().count() <= 50);
        }
        for pair in pieces.windows(2) {
            assert_eq!(pair[0].end, pair[1].start);
        }
        assert_eq!(pieces[0].text, "开头。");
    }
    #[test]
    fn sentences_and_spoken_characters_bound_chinese_duration() {
        let text = "一句话。第二句话！第三句话？";
        let pieces = split(text, |_| Ok(1)).unwrap();
        assert_eq!(
            pieces
                .iter()
                .map(|piece| piece.text.as_str())
                .collect::<Vec<_>>(),
            vec![text]
        );
        let paragraphs = split("第一句。第二句。\n第三句。", |_| Ok(1)).unwrap();
        assert_eq!(paragraphs.len(), 1);
        assert_eq!(paragraphs[0].text, "第一句。第二句。\n第三句。");
        // Once the budget is exceeded, prefer a complete sentence over a
        // later comma, leaving the next sentence's context intact.
        let pieces = split(
            &format!("一句话。{}，{}。", "中".repeat(35), "文".repeat(35)),
            |_| Ok(1),
        )
        .unwrap();
        assert_eq!(pieces[0].text, "一句话。");
        // A BPE token can encode multiple Chinese characters. Even a low token
        // count must not merge a long sentence beyond the speech budget.
        let text = "中文".repeat(100);
        let pieces = split(&text, |_| Ok(1)).unwrap();
        assert!(pieces.iter().all(|piece| piece.text.chars().count() <= 60));
        assert_eq!(
            pieces
                .iter()
                .map(|piece| piece.text.as_str())
                .collect::<String>(),
            text
        );
    }

    #[test]
    fn decorations_do_not_enter_synthesis_or_shift_source_ranges() {
        let text = format!("{}\r\n正文 a=b。\n----\n继续阅读。", "=".repeat(200));
        let pieces = split(&text, |s| Ok(s.chars().count())).unwrap();
        assert_eq!(pieces.len(), 2);
        for piece in &pieces {
            assert_eq!(&text[piece.start..piece.end], piece.text);
            assert!(!piece.text.contains("===="));
        }
        assert_eq!(normalize("====\n正文 a=b。\n----"), "正文 a=b。");
        assert_eq!(normalize("===="), "");
    }

    #[test]
    fn normalization_is_separate_from_source() {
        assert_eq!(normalize("你好\r\n世界"), "你好世界。");
        assert_eq!(normalize("こんにちは"), "こんにちは。");
        assert_eq!(normalize("hello friend"), "        Hello friend.");
        assert!(split("\r\n   ", |s| Ok(s.len())).unwrap().is_empty());
    }
}
