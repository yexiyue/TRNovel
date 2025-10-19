use super::{Analyzer, HtmlAnalyzer};
use crate::Result;
use anyhow::anyhow;
use regex::Regex;
use std::{collections::HashMap, sync::LazyLock};

pub struct DefaultAnalyzer {
    analyzer: HtmlAnalyzer,
}

static CLASS_MAP: LazyLock<HashMap<&'static str, &'static str>> =
    LazyLock::new(|| HashMap::from_iter(vec![("class", "."), ("id", "#"), ("tag", "")]));
static RANGE_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\[(.*?)\]").unwrap());

fn rule_to_selector(rule: &str) -> Result<String> {
    let mut selectors = vec![];
    let segments = rule.split("@").collect::<Vec<_>>();
    let len = segments.len();
    for (index, segment) in segments.into_iter().enumerate() {
        if index == len - 1 && !segment.contains(".") {
            selectors.push(format!("@{}", segment));
            continue;
        }
        let mut segment = segment.trim();
        let mut position_str = "";
        let mut res = String::new();

        if let Some(range) = RANGE_RE.find(segment) {
            segment = &segment[..range.start()];
            position_str = range.as_str()[1..range.as_str().len() - 1].trim();
        }

        let parts = segment.split('.').collect::<Vec<_>>();

        match parts.len() {
            1 => {
                res.push_str(parts[0]);
            }
            2 => {
                let value = parts[1];
                let class = CLASS_MAP.get(parts[0]).unwrap_or(&"");
                res.push_str(&format!("{}{}", class, value));
            }
            3 => {
                let value = parts[1];
                let class = CLASS_MAP.get(parts[0]).unwrap_or(&"");
                let position = parts[2].parse::<usize>()? + 1;
                res.push_str(&format!("{}{}:nth-of-type({})", class, value, position));
            }
            _ => {
                return Err(anyhow!("Invalid rule: {}", segment).into());
            }
        }

        if !position_str.is_empty() {
            let mut range_res = vec![];
            let mut is_exclude = false;

            if position_str.contains("=") {
                let (property_name, property_value) = position_str.split_once("=").unwrap();
                res = format!(r#"{}[{}="{}"]"#, res, property_name, property_value);
                selectors.push(res);
                continue;
            } else if position_str.starts_with("!") {
                position_str = &position_str[1..];
                is_exclude = true;
            }

            for i in position_str.split(",") {
                if i.contains(":") {
                    let range = i.split(":").collect::<Vec<_>>();
                    let start = range[0].parse::<isize>()? + 1;
                    let end = range[1].parse::<isize>()? + 1;
                    let step = range.get(2).unwrap_or(&"");
                    range_res.push(format!(
                        ":nth-of-type({step}n+{start}):not(:nth-of-type({step}n+{end}))"
                    ));
                } else {
                    let position = i.parse::<isize>()? + 1;
                    if position < 0 {
                        range_res.push(format!(":nth-last-of-type({})", position.abs()));
                    } else {
                        range_res.push(format!(":nth-of-type({})", position));
                    }
                }
            }

            if is_exclude {
                res = format!("{}:not({})", res, range_res.join(","));
            } else {
                res = format!("{}:is({})", res, range_res.join(","));
            }
        }
        selectors.push(res);
    }
    Ok(selectors.join(" "))
}

impl Analyzer for DefaultAnalyzer {
    fn parse(content: &str) -> Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            analyzer: HtmlAnalyzer::parse(content)?,
        })
    }

    fn get_string(&self, rule: &str) -> Result<String> {
        let selector = rule_to_selector(rule)?;
        self.analyzer.get_string(&selector)
    }

    fn get_elements(&self, rule: &str) -> Result<Vec<String>> {
        let selector = rule_to_selector(rule)?;
        self.analyzer.get_elements(&selector)
    }
}

#[cfg(test)]
mod test {

    use super::*;

    #[test]
    fn test_rule_to_selector() {
        assert_eq!(
            ".result-game-item-info p:nth-of-type(1) span:nth-of-type(2) @text",
            rule_to_selector("class.result-game-item-info@tag.p.0@tag.span.1@text").unwrap()
        );

        assert_eq!(
            "#intro p:nth-of-type(1) @text",
            rule_to_selector("id.intro@tag.p.0@text").unwrap()
        );

        assert_eq!(".bookbox", rule_to_selector("class.bookbox").unwrap());

        assert_eq!(
            "#fmimg img @src",
            rule_to_selector("id.fmimg@img@src").unwrap()
        );

        assert_eq!(
            "[property=\"og:novel:update_time\"] @content",
            rule_to_selector("[property=og:novel:update_time]@content").unwrap()
        );

        assert_eq!(
            ".bookbox:is(:nth-of-type(2),:nth-of-type(5),:nth-of-type(4))",
            rule_to_selector("class.bookbox[1,4,3]").unwrap()
        );

        assert_eq!(
            ".bookbox:not(:nth-of-type(2),:nth-of-type(5),:nth-of-type(4))",
            rule_to_selector("class.bookbox[!1,4,3]").unwrap()
        );

        assert_eq!(
            ".bookbox:is(:nth-of-type(n+4):not(:nth-of-type(n+11)))",
            rule_to_selector("class.bookbox[3:10]").unwrap()
        );
    }

    #[test]
    fn test_default_analyzer_get_string() {
        let analyzer =
            DefaultAnalyzer::parse(r#"<li><a href="/xuanhuan/">玄幻小说</a></li>"#).unwrap();
        let res = analyzer.get_string("tag.a@href").unwrap();
        assert_eq!(res, "/xuanhuan/");
    }
}
