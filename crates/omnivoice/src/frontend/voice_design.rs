use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

use strsim::normalized_levenshtein;

use crate::error::{OmniVoiceError, Result};

static INSTRUCT_EN_TO_ZH: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static INSTRUCT_ZH_TO_EN: OnceLock<HashMap<&'static str, &'static str>> = OnceLock::new();
static INSTRUCT_MUTUALLY_EXCLUSIVE: OnceLock<Vec<HashSet<&'static str>>> = OnceLock::new();
static INSTRUCT_ALL_VALID: OnceLock<HashSet<&'static str>> = OnceLock::new();
static INSTRUCT_VALID_EN: OnceLock<Vec<&'static str>> = OnceLock::new();
static INSTRUCT_VALID_ZH: OnceLock<Vec<&'static str>> = OnceLock::new();

const DIALECTS: &[&str] = &[
    "河南话",
    "陕西话",
    "四川话",
    "贵州话",
    "云南话",
    "桂林话",
    "济南话",
    "石家庄话",
    "甘肃话",
    "宁夏话",
    "青岛话",
    "东北话",
];

const ACCENTS: &[&str] = &[
    "american accent",
    "british accent",
    "australian accent",
    "chinese accent",
    "canadian accent",
    "indian accent",
    "korean accent",
    "portuguese accent",
    "russian accent",
    "japanese accent",
];

const TRANSLATED_CATEGORIES: &[&[(&str, &str)]] = &[
    &[("male", "男"), ("female", "女")],
    &[
        ("child", "儿童"),
        ("teenager", "少年"),
        ("young adult", "青年"),
        ("middle-aged", "中年"),
        ("elderly", "老年"),
    ],
    &[
        ("very low pitch", "极低音调"),
        ("low pitch", "低音调"),
        ("moderate pitch", "中音调"),
        ("high pitch", "高音调"),
        ("very high pitch", "极高音调"),
    ],
    &[("whisper", "耳语")],
];

pub fn contains_cjk(text: &str) -> bool {
    text.chars()
        .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch))
}

pub fn resolve_instruct(instruct: Option<&str>, use_zh: bool) -> Result<Option<String>> {
    let Some(instruct) = instruct else {
        return Ok(None);
    };

    let instruct = instruct.trim();
    if instruct.is_empty() {
        return Ok(None);
    }

    let raw_items: Vec<&str> = instruct
        .split(&[',', '，'][..])
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .collect();

    let mut unknown = Vec::new();
    let mut normalized = Vec::with_capacity(raw_items.len());
    for raw in raw_items {
        let lowered = raw.to_lowercase();
        if instruct_all_valid().contains(lowered.as_str()) {
            normalized.push(lowered);
        } else {
            let suggestion = closest_instruct_match(&lowered);
            unknown.push((raw.to_string(), lowered, suggestion));
        }
    }

    if !unknown.is_empty() {
        let mut lines = Vec::with_capacity(unknown.len());
        for (raw, lowered, suggestion) in unknown {
            if let Some(suggestion) = suggestion {
                lines.push(format!(
                    "  '{raw}' -> '{lowered}' (unsupported; did you mean '{suggestion}'?)"
                ));
            } else {
                lines.push(format!("  '{raw}' -> '{lowered}' (unsupported)"));
            }
        }

        let mut valid_en = instruct_valid_en().to_vec();
        valid_en.sort_unstable();
        let mut valid_zh = instruct_valid_zh().to_vec();
        valid_zh.sort_unstable();

        return Err(OmniVoiceError::InvalidRequest(format!(
            "Unsupported instruct items found in {instruct}:\n{}\n\nValid English items: {}\nValid Chinese items: {}\n\nTip: Use only English or only Chinese instructs. English instructs should use comma + space (e.g. 'male, indian accent'),\nChinese instructs should use full-width comma (e.g. '男，河南话').",
            lines.join("\n"),
            valid_en.join(", "),
            valid_zh.join("，"),
        )));
    }

    let has_dialect = normalized.iter().any(|item| item.ends_with('话'));
    let has_accent = normalized.iter().any(|item| item.contains(" accent"));

    if has_dialect && has_accent {
        return Err(OmniVoiceError::InvalidRequest(
            "Cannot mix Chinese dialect and English accent in a single instruct. Dialects are for Chinese speech, accents for English speech.".to_string(),
        ));
    }

    let target_zh = if has_dialect {
        true
    } else if has_accent {
        false
    } else {
        use_zh
    };

    let unified: Vec<String> = if target_zh {
        normalized
            .into_iter()
            .map(|item| {
                instruct_en_to_zh()
                    .get(item.as_str())
                    .copied()
                    .unwrap_or(item.as_str())
                    .to_string()
            })
            .collect()
    } else {
        normalized
            .into_iter()
            .map(|item| {
                instruct_zh_to_en()
                    .get(item.as_str())
                    .copied()
                    .unwrap_or(item.as_str())
                    .to_string()
            })
            .collect()
    };

    let mut conflicts = Vec::new();
    for category in instruct_mutually_exclusive() {
        let hits: Vec<&str> = unified
            .iter()
            .filter_map(|item| category.contains(item.as_str()).then_some(item.as_str()))
            .collect();
        if hits.len() > 1 {
            conflicts.push(
                hits.iter()
                    .map(|item| format!("'{item}'"))
                    .collect::<Vec<_>>()
                    .join(" vs "),
            );
        }
    }

    if !conflicts.is_empty() {
        return Err(OmniVoiceError::InvalidRequest(format!(
            "Conflicting instruct items within the same category: {}. Each category (gender, age, pitch, style, accent, dialect) allows at most one item.",
            conflicts.join("; ")
        )));
    }

    let separator = if unified.iter().any(|item| contains_cjk(item)) {
        "，"
    } else {
        ", "
    };

    Ok(Some(unified.join(separator)))
}

fn closest_instruct_match(needle: &str) -> Option<&'static str> {
    instruct_all_valid()
        .iter()
        .copied()
        .map(|candidate| (candidate, normalized_levenshtein(needle, candidate)))
        .filter(|(_, score)| *score >= 0.6)
        .max_by(|(_, left), (_, right)| {
            left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal)
        })
        .map(|(candidate, _)| candidate)
}

fn instruct_en_to_zh() -> &'static HashMap<&'static str, &'static str> {
    INSTRUCT_EN_TO_ZH.get_or_init(|| {
        let mut mapping = HashMap::new();
        for category in TRANSLATED_CATEGORIES {
            for (en, zh) in *category {
                mapping.insert(*en, *zh);
            }
        }
        mapping
    })
}

fn instruct_zh_to_en() -> &'static HashMap<&'static str, &'static str> {
    INSTRUCT_ZH_TO_EN.get_or_init(|| {
        instruct_en_to_zh()
            .iter()
            .map(|(en, zh)| (*zh, *en))
            .collect()
    })
}

fn instruct_mutually_exclusive() -> &'static Vec<HashSet<&'static str>> {
    INSTRUCT_MUTUALLY_EXCLUSIVE.get_or_init(|| {
        let mut categories = Vec::new();
        for category in TRANSLATED_CATEGORIES {
            let mut items = HashSet::new();
            for (en, zh) in *category {
                items.insert(*en);
                items.insert(*zh);
            }
            categories.push(items);
        }
        categories.push(ACCENTS.iter().copied().collect());
        categories.push(DIALECTS.iter().copied().collect());
        categories
    })
}

fn instruct_all_valid() -> &'static HashSet<&'static str> {
    INSTRUCT_ALL_VALID.get_or_init(|| {
        let mut valid = HashSet::new();
        valid.extend(instruct_en_to_zh().keys().copied());
        valid.extend(instruct_zh_to_en().keys().copied());
        valid.extend(ACCENTS.iter().copied());
        valid.extend(DIALECTS.iter().copied());
        valid
    })
}

fn instruct_valid_en() -> &'static Vec<&'static str> {
    INSTRUCT_VALID_EN.get_or_init(|| {
        let mut items: Vec<_> = instruct_all_valid()
            .iter()
            .copied()
            .filter(|item| !contains_cjk(item))
            .collect();
        items.sort_unstable();
        items
    })
}

fn instruct_valid_zh() -> &'static Vec<&'static str> {
    INSTRUCT_VALID_ZH.get_or_init(|| {
        let mut items: Vec<_> = instruct_all_valid()
            .iter()
            .copied()
            .filter(|item| contains_cjk(item))
            .collect();
        items.sort_unstable();
        items
    })
}
