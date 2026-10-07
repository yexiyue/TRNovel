//! Cn2An and Emilia English normalization order, before phonemization.
use regex::{Captures, Regex};
fn replace(text: &str, pattern: &str, replacement: impl Fn(&Captures<'_>) -> String) -> String {
    Regex::new(pattern)
        .expect("fixed frontend expression")
        .replace_all(text, replacement)
        .into_owned()
}
const DIGITS: [char; 10] = ['零', '一', '二', '三', '四', '五', '六', '七', '八', '九'];
fn direct(number: &str) -> String {
    number
        .bytes()
        .map(|b| DIGITS[(b - b'0') as usize])
        .collect()
}
fn chinese_number(number: &str) -> String {
    let negative = number.starts_with('-');
    let unsigned = number.trim_start_matches('-');
    let (integer, decimal) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let integer = integer.trim_start_matches('0');
    if integer.len() > 16 {
        return number.into();
    }
    let units = [
        "", "十", "百", "千", "万", "十", "百", "千", "亿", "十", "百", "千", "万", "十", "百",
        "千",
    ];
    let mut result = String::new();
    for (index, digit) in integer.bytes().enumerate() {
        let position = integer.len() - index - 1;
        if digit != b'0' {
            result.push(DIGITS[(digit - b'0') as usize]);
            result.push_str(units[position]);
        } else {
            if position.is_multiple_of(4) {
                result.push('零');
                result.push_str(units[position]);
            }
            if index > 0 && !result.ends_with('零') {
                result.push('零');
            }
        }
    }
    result = result
        .replace("零零", "零")
        .replace("零万", "万")
        .replace("零亿", "亿")
        .replace("亿万", "亿")
        .trim_matches('零')
        .into();
    result = replace(
        &result,
        r"([万亿])零([一二三四五六七八九]千)",
        |c| format!("{}{}", &c[1], &c[2]),
    );
    if result.starts_with("一十") {
        result = result.trim_start_matches('一').into();
    }
    if result.is_empty() {
        result.push('零');
    }
    if !decimal.is_empty() {
        result.push('点');
        result.push_str(&direct(&decimal[..decimal.len().min(16)]));
    }
    if negative {
        result.insert(0, '负');
    }
    result
}
pub fn chinese(text: &str) -> String {
    let measurements = "斤|克|千克|公斤|吨|米|厘米|毫米|公里|升|毫升|元|角|分|个|只|条|张|块|瓶|杯|份|本|辆|台|匹|头|位|亩|小时|分钟|秒|天|半";
    let mut value = replace(
        text,
        &format!(r"(\d+(?:\.\d+)?)-(\d+(?:\.\d+)?)({measurements})"),
        |c| {
            format!(
                "{}到{}{}",
                chinese_number(&c[1]),
                chinese_number(&c[2]),
                &c[3]
            )
        },
    );
    value = replace(&value, r"(\d{2,4})年", |c| format!("{}年", direct(&c[1])));
    value = replace(&value, r"(\d{1,2})(月|日)", |c| {
        format!("{}{}", chinese_number(&c[1]), &c[2])
    });
    value = replace(&value, r"(\d+)/(\d+)", |c| {
        format!("{}分之{}", chinese_number(&c[2]), chinese_number(&c[1]))
    });
    value = replace(&value, r"(-?(?:\d+\.)?\d+)%", |c| {
        format!("百分之{}", chinese_number(&c[1]))
    });
    value = replace(&value, r"(\d+)℃", |c| {
        format!("{}摄氏度", chinese_number(&c[1]))
    });
    replace(&value, r"-?(?:\d+\.)?\d+", |c| chinese_number(&c[0]))
}

const SMALL: [&str; 20] = [
    "zero",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
fn cardinal(number: u64) -> String {
    if number < 20 {
        return SMALL[number as usize].into();
    }
    if number < 100 {
        return if number.is_multiple_of(10) {
            TENS[(number / 10) as usize].into()
        } else {
            format!(
                "{}-{}",
                TENS[(number / 10) as usize],
                SMALL[(number % 10) as usize]
            )
        };
    }
    if number < 1000 {
        return format!(
            "{} hundred{}",
            SMALL[(number / 100) as usize],
            if number.is_multiple_of(100) {
                String::new()
            } else {
                format!(" {}", cardinal(number % 100))
            }
        );
    }
    for (scale, name) in [
        (1_000_000_000_000_000_000, "quintillion"),
        (1_000_000_000_000_000, "quadrillion"),
        (1_000_000_000_000, "trillion"),
        (1_000_000_000, "billion"),
        (1_000_000, "million"),
        (1000, "thousand"),
    ] {
        if number >= scale {
            return format!(
                "{} {name}{}",
                cardinal(number / scale),
                if number.is_multiple_of(scale) {
                    String::new()
                } else {
                    format!(", {}", cardinal(number % scale))
                }
            );
        }
    }
    unreachable!("large number has a scale")
}
fn ordinal(number: u64) -> String {
    let value = cardinal_with_and(number);
    let (prefix, last) = value
        .rsplit_once([' ', '-'])
        .map_or(("", value.as_str()), |(prefix, last)| {
            (&value[..prefix.len() + 1], last)
        });
    let last = match last {
        "one" => "first".into(),
        "two" => "second".into(),
        "three" => "third".into(),
        "five" => "fifth".into(),
        "eight" => "eighth".into(),
        "nine" => "ninth".into(),
        "twelve" => "twelfth".into(),
        v if v.ends_with('y') => format!("{}ieth", &v[..v.len() - 1]),
        v => format!("{v}th"),
    };
    format!("{prefix}{last}")
}
fn cardinal_with_and(number: u64) -> String {
    // inflect's default conjunction is used for fractions and ordinals only.
    if number < 100 {
        return cardinal(number);
    }
    if number < 1000 {
        return format!(
            "{} hundred{}",
            cardinal(number / 100),
            if number.is_multiple_of(100) {
                String::new()
            } else {
                format!(" and {}", cardinal(number % 100))
            }
        );
    }
    for (scale, name) in [
        (1_000_000_000_000_000_000, "quintillion"),
        (1_000_000_000_000_000, "quadrillion"),
        (1_000_000_000_000, "trillion"),
        (1_000_000_000, "billion"),
        (1_000_000, "million"),
        (1000, "thousand"),
    ] {
        if number >= scale {
            let remainder = number % scale;
            let suffix = if remainder == 0 {
                String::new()
            } else {
                format!(
                    "{}{}",
                    if remainder < 100 { " and " } else { ", " },
                    cardinal_with_and(remainder)
                )
            };
            return format!("{} {name}{suffix}", cardinal_with_and(number / scale));
        }
    }
    unreachable!("large number has a scale")
}
fn english_number(number: &str) -> String {
    let Ok(value) = number.parse::<u64>() else {
        return number.into();
    };
    if (1001..3000).contains(&value) {
        if value == 2000 {
            return "two thousand".into();
        }
        if (2001..2010).contains(&value) {
            return format!("two thousand {}", cardinal(value % 100));
        }
        if value.is_multiple_of(100) {
            return format!("{} hundred", cardinal(value / 100));
        }
        let last = if value % 100 < 10 {
            format!("oh {}", cardinal(value % 100))
        } else {
            cardinal(value % 100)
        };
        return format!("{} {last}", cardinal(value / 100));
    }
    cardinal(value)
}
pub fn english(text: &str) -> String {
    let mut value = text.to_string();
    for (short, long) in [
        ("mrs", "misess"),
        ("mr", "mister"),
        ("dr", "doctor"),
        ("st", "saint"),
        ("co", "company"),
        ("jr", "junior"),
        ("maj", "major"),
        ("gen", "general"),
        ("drs", "doctors"),
        ("rev", "reverend"),
        ("lt", "lieutenant"),
        ("hon", "honorable"),
        ("sgt", "sergeant"),
        ("capt", "captain"),
        ("esq", "esquire"),
        ("ltd", "limited"),
        ("col", "colonel"),
        ("ft", "fort"),
        ("etc", "et cetera"),
        ("btw", "by the way"),
    ] {
        value = Regex::new(&format!(r"(?i)\b{short}\b"))
            .expect("abbreviation")
            .replace_all(&value, long)
            .into_owned();
    }
    value = replace(&value, r"([0-9][0-9,]+[0-9])", |c| c[1].replace(',', ""));
    value = replace(&value, r"£([0-9,]*[0-9]+)", |c| {
        format!("{} pounds", &c[1])
    });
    value = replace(&value, r"\$([0-9.,]*[0-9]+)", |c| {
        let pieces: Vec<_> = c[1].split('.').collect();
        if pieces.len() > 2 {
            return format!("{} dollars ", &c[1]);
        }
        let dollars = pieces[0].parse::<u64>().unwrap_or(0);
        let cents = pieces
            .get(1)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0);
        match (dollars, cents) {
            (0, 0) => " zero dollars ".into(),
            (d, 0) => format!(" {d} {} ", if d == 1 { "dollar" } else { "dollars" }),
            (0, c) => format!(" {c} {} ", if c == 1 { "cent" } else { "cents" }),
            (d, c) => format!(
                " {d} {}, {c} {} ",
                if d == 1 { "dollar" } else { "dollars" },
                if c == 1 { "cent" } else { "cents" }
            ),
        }
    });
    value = replace(&value, r"(\d+)/(\d+)", |c| {
        let (Ok(numerator), Ok(denominator)) = (c[1].parse::<u64>(), c[2].parse::<u64>()) else {
            return c[0].into();
        };
        let phrase = match (numerator, denominator) {
            (1, 2) => "one half".into(),
            (1, 4) => "one quarter".into(),
            (n, 2) => format!("{} halves", cardinal_with_and(n)),
            (n, 4) => format!("{} quarters", cardinal_with_and(n)),
            (n, d) => format!("{} {}", cardinal_with_and(n), ordinal(d)),
        };
        format!(" {phrase} ")
    });
    value = replace(&value, r"([0-9]+\.[0-9]+)", |c| {
        c[1].replace('.', " point ")
    });
    value = replace(&value, r"([0-9.,]*[0-9]+%)", |c| {
        c[1].replace('%', " percent ")
    });
    value = replace(&value, r"([0-9]+)(st|nd|rd|th)", |c| {
        c[1].parse::<u64>()
            .map_or_else(|_| c[0].into(), |v| format!(" {} ", ordinal(v)))
    });
    replace(&value, r"[0-9]+", |c| {
        format!(" {} ", english_number(&c[0]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn date_fraction_measurement_and_decimal_keep_upstream_order() {
        assert_eq!(
            chinese("2026年10月7日，1/2，12.5%，3-5本，25℃，-0.25。"),
            "二零二六年十月七日，二分之一，百分之十二点五，三到五本，二十五摄氏度，负零点二五。"
        );
        assert_eq!(chinese_number("10001"), "一万零一");
        assert_eq!(chinese_number("100000000"), "一亿");
    }
}
