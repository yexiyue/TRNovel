use std::sync::LazyLock;

use regex::Regex;

// let stop = Regex::new(r"[。！？!?，；：,;:、]").unwrap();

static STOP_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[。！？!?]").unwrap());
static SUB_REGEX: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[,;:，；：]").unwrap());

/// 预处理文本，将长文本拆分为适合TTS处理的短句
pub fn preprocess_text_recursive(text: &str, limit: usize, regex: &Regex) -> Vec<String> {
    let mut res = vec![];

    for line in text.lines() {
        if line.trim().is_empty() {
            continue;
        }
        if line.len() <= limit {
            res.push(line.trim().to_string());
            continue;
        }
        let mut start = 0;
        for item in regex.find_iter(line) {
            let end = item.end();
            let current = line[start..end].trim().to_string();
            if current.len() > limit && regex.as_str() != SUB_REGEX.as_str() {
                let mut sub_res = preprocess_text_recursive(&current, limit, &SUB_REGEX);
                res.append(&mut sub_res);
            } else {
                res.push(current);
            }
            start = end;
        }
        if start < line.len() {
            let current = line[start..].trim().to_string();
            if current.len() > limit && regex.as_str() != SUB_REGEX.as_str() {
                let mut sub_res = preprocess_text_recursive(&current, limit, &SUB_REGEX);
                res.append(&mut sub_res);
            } else {
                res.push(current);
            }
        }
    }

    res
}

// todo 优化，返回每段文字在原始位置的byte位置方便高亮
pub fn preprocess_text(text: &str, limit: usize) -> Vec<String> {
    preprocess_text_recursive(text, limit, &STOP_REGEX)
}
