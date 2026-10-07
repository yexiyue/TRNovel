use unicode_general_category::{GeneralCategory, get_general_category};

#[derive(Clone, Debug, Default)]
pub struct RuleDurationEstimator;

const CJK_WEIGHT: f32 = 3.0;
const HANGUL_WEIGHT: f32 = 2.5;
const KANA_WEIGHT: f32 = 2.2;
const ETHIOPIC_WEIGHT: f32 = 3.0;
const YI_WEIGHT: f32 = 3.0;
const INDIC_WEIGHT: f32 = 1.8;
const THAI_LAO_WEIGHT: f32 = 1.5;
const KHMER_MYANMAR_WEIGHT: f32 = 1.8;
const ARABIC_WEIGHT: f32 = 1.5;
const HEBREW_WEIGHT: f32 = 1.5;
const LATIN_WEIGHT: f32 = 1.0;
const CYRILLIC_WEIGHT: f32 = 1.0;
const GREEK_WEIGHT: f32 = 1.0;
const ARMENIAN_WEIGHT: f32 = 1.0;
const GEORGIAN_WEIGHT: f32 = 1.0;
const PUNCTUATION_WEIGHT: f32 = 0.5;
const SPACE_WEIGHT: f32 = 0.2;
const DIGIT_WEIGHT: f32 = 3.5;
const MARK_WEIGHT: f32 = 0.0;
const DEFAULT_WEIGHT: f32 = 1.0;

const RANGES: &[(u32, f32)] = &[
    (0x02AF, LATIN_WEIGHT),
    (0x03FF, GREEK_WEIGHT),
    (0x052F, CYRILLIC_WEIGHT),
    (0x058F, ARMENIAN_WEIGHT),
    (0x05FF, HEBREW_WEIGHT),
    (0x077F, ARABIC_WEIGHT),
    (0x089F, ARABIC_WEIGHT),
    (0x08FF, ARABIC_WEIGHT),
    (0x097F, INDIC_WEIGHT),
    (0x09FF, INDIC_WEIGHT),
    (0x0A7F, INDIC_WEIGHT),
    (0x0AFF, INDIC_WEIGHT),
    (0x0B7F, INDIC_WEIGHT),
    (0x0BFF, INDIC_WEIGHT),
    (0x0C7F, INDIC_WEIGHT),
    (0x0CFF, INDIC_WEIGHT),
    (0x0D7F, INDIC_WEIGHT),
    (0x0DFF, INDIC_WEIGHT),
    (0x0EFF, THAI_LAO_WEIGHT),
    (0x0FFF, INDIC_WEIGHT),
    (0x109F, KHMER_MYANMAR_WEIGHT),
    (0x10FF, GEORGIAN_WEIGHT),
    (0x11FF, HANGUL_WEIGHT),
    (0x137F, ETHIOPIC_WEIGHT),
    (0x139F, ETHIOPIC_WEIGHT),
    (0x13FF, DEFAULT_WEIGHT),
    (0x167F, DEFAULT_WEIGHT),
    (0x169F, DEFAULT_WEIGHT),
    (0x16FF, DEFAULT_WEIGHT),
    (0x171F, DEFAULT_WEIGHT),
    (0x173F, DEFAULT_WEIGHT),
    (0x175F, DEFAULT_WEIGHT),
    (0x177F, DEFAULT_WEIGHT),
    (0x17FF, KHMER_MYANMAR_WEIGHT),
    (0x18AF, DEFAULT_WEIGHT),
    (0x18FF, DEFAULT_WEIGHT),
    (0x194F, INDIC_WEIGHT),
    (0x19DF, INDIC_WEIGHT),
    (0x19FF, KHMER_MYANMAR_WEIGHT),
    (0x1A1F, INDIC_WEIGHT),
    (0x1AAF, INDIC_WEIGHT),
    (0x1B7F, INDIC_WEIGHT),
    (0x1BBF, INDIC_WEIGHT),
    (0x1BFF, INDIC_WEIGHT),
    (0x1C4F, INDIC_WEIGHT),
    (0x1C7F, INDIC_WEIGHT),
    (0x1C8F, CYRILLIC_WEIGHT),
    (0x1CBF, GEORGIAN_WEIGHT),
    (0x1CCF, INDIC_WEIGHT),
    (0x1CFF, INDIC_WEIGHT),
    (0x1D7F, LATIN_WEIGHT),
    (0x1DBF, LATIN_WEIGHT),
    (0x1DFF, DEFAULT_WEIGHT),
    (0x1EFF, LATIN_WEIGHT),
    (0x309F, KANA_WEIGHT),
    (0x30FF, KANA_WEIGHT),
    (0x312F, CJK_WEIGHT),
    (0x318F, HANGUL_WEIGHT),
    (0x9FFF, CJK_WEIGHT),
    (0xA4CF, YI_WEIGHT),
    (0xA4FF, DEFAULT_WEIGHT),
    (0xA63F, DEFAULT_WEIGHT),
    (0xA69F, CYRILLIC_WEIGHT),
    (0xA6FF, DEFAULT_WEIGHT),
    (0xA7FF, LATIN_WEIGHT),
    (0xA82F, INDIC_WEIGHT),
    (0xA87F, DEFAULT_WEIGHT),
    (0xA8DF, INDIC_WEIGHT),
    (0xA8FF, INDIC_WEIGHT),
    (0xA92F, INDIC_WEIGHT),
    (0xA95F, INDIC_WEIGHT),
    (0xA97F, HANGUL_WEIGHT),
    (0xA9DF, INDIC_WEIGHT),
    (0xA9FF, KHMER_MYANMAR_WEIGHT),
    (0xAA5F, INDIC_WEIGHT),
    (0xAA7F, KHMER_MYANMAR_WEIGHT),
    (0xAADF, INDIC_WEIGHT),
    (0xAAFF, INDIC_WEIGHT),
    (0xAB2F, ETHIOPIC_WEIGHT),
    (0xAB6F, LATIN_WEIGHT),
    (0xABBF, DEFAULT_WEIGHT),
    (0xABFF, INDIC_WEIGHT),
    (0xD7AF, HANGUL_WEIGHT),
    (0xFAFF, CJK_WEIGHT),
    (0xFDFF, ARABIC_WEIGHT),
    (0xFE6F, DEFAULT_WEIGHT),
    (0xFEFF, ARABIC_WEIGHT),
    (0xFFEF, LATIN_WEIGHT),
];

impl RuleDurationEstimator {
    pub fn estimate_duration(
        &self,
        target_text: &str,
        ref_text: &str,
        ref_duration: f32,
        low_threshold: Option<f32>,
        boost_strength: f32,
    ) -> f32 {
        if ref_duration <= 0.0 || ref_text.is_empty() {
            return 0.0;
        }

        let ref_weight = self.calculate_total_weight(ref_text);
        if ref_weight == 0.0 {
            return 0.0;
        }

        let speed_factor = ref_weight / ref_duration;
        let target_weight = self.calculate_total_weight(target_text);
        let estimated_duration = target_weight / speed_factor;

        if let Some(low_threshold) = low_threshold.filter(|value| estimated_duration < *value) {
            let alpha = 1.0 / boost_strength;
            low_threshold * (estimated_duration / low_threshold).powf(alpha)
        } else {
            estimated_duration
        }
    }

    pub fn calculate_total_weight(&self, text: &str) -> f32 {
        text.chars().map(char_weight).sum()
    }
}

fn char_weight(ch: char) -> f32 {
    let code = ch as u32;
    if ch.is_ascii_alphabetic() {
        return LATIN_WEIGHT;
    }
    if code == 32 {
        return SPACE_WEIGHT;
    }
    if code == 0x0640 {
        return MARK_WEIGHT;
    }

    match get_general_category(ch) {
        GeneralCategory::NonspacingMark
        | GeneralCategory::SpacingMark
        | GeneralCategory::EnclosingMark => return MARK_WEIGHT,
        GeneralCategory::ConnectorPunctuation
        | GeneralCategory::DashPunctuation
        | GeneralCategory::OpenPunctuation
        | GeneralCategory::ClosePunctuation
        | GeneralCategory::InitialPunctuation
        | GeneralCategory::FinalPunctuation
        | GeneralCategory::OtherPunctuation
        | GeneralCategory::MathSymbol
        | GeneralCategory::CurrencySymbol
        | GeneralCategory::ModifierSymbol
        | GeneralCategory::OtherSymbol => return PUNCTUATION_WEIGHT,
        GeneralCategory::SpaceSeparator
        | GeneralCategory::LineSeparator
        | GeneralCategory::ParagraphSeparator => return SPACE_WEIGHT,
        GeneralCategory::DecimalNumber
        | GeneralCategory::LetterNumber
        | GeneralCategory::OtherNumber => return DIGIT_WEIGHT,
        _ => {}
    }

    let index = RANGES.partition_point(|(end, _)| *end < code);
    if let Some((_, weight)) = RANGES.get(index) {
        return *weight;
    }
    if code > 0x20_000 {
        return CJK_WEIGHT;
    }
    DEFAULT_WEIGHT
}
