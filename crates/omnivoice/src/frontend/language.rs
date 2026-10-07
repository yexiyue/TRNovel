use std::{
    collections::{HashMap, HashSet},
    sync::OnceLock,
};

const LANG_ID_NAME_MAP_TSV: &str = include_str!("lang_id_name_map.tsv");

static LANG_NAME_TO_ID: OnceLock<HashMap<String, String>> = OnceLock::new();
static LANG_IDS: OnceLock<HashSet<String>> = OnceLock::new();

pub fn resolve_language(language: Option<&str>) -> Option<String> {
    let language = language?.trim();
    if language.is_empty() || language.eq_ignore_ascii_case("none") {
        return None;
    }
    let normalized = language.to_ascii_lowercase();
    if lang_ids().contains(&normalized) {
        return Some(normalized);
    }
    lang_name_to_id().get(normalized.as_str()).cloned()
}

fn lang_name_to_id() -> &'static HashMap<String, String> {
    LANG_NAME_TO_ID.get_or_init(|| {
        let mut mapping = HashMap::with_capacity(700);
        for line in LANG_ID_NAME_MAP_TSV
            .lines()
            .skip(1)
            .filter(|line| !line.is_empty())
        {
            let mut fields = line.split('\t');
            let Some(language_id) = fields.next() else {
                continue;
            };
            let Some(language_name) = fields.next() else {
                continue;
            };
            mapping.insert(language_name.to_ascii_lowercase(), language_id.to_string());
        }
        mapping
    })
}

fn lang_ids() -> &'static HashSet<String> {
    LANG_IDS.get_or_init(|| {
        let mut ids = HashSet::with_capacity(700);
        for line in LANG_ID_NAME_MAP_TSV
            .lines()
            .skip(1)
            .filter(|line| !line.is_empty())
        {
            if let Some(language_id) = line.split('\t').next() {
                ids.insert(language_id.to_string());
            }
        }
        ids
    })
}
