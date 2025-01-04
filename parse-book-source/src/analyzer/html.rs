use super::Analyzer;
use anyhow::anyhow;
use regex::Regex;
use scraper::{Html, Selector};

fn html_decode(s: &str) -> String {
    let mut result = s.replace("&amp;", "&");
    result = result.replace("&lt;", "<");
    result = result.replace("&gt;", ">");
    result = result.replace("&nbsp;", " ");
    result = result.replace("&#39;", "'");
    result = result.replace("&quot;", "\"");
    result = result.replace("<br/>", "\n");
    result
}

fn get_html_string(html: &str) -> String {
    let re_tags = Regex::new(r"</?(?:div|p|br|hr|h\d|article|b|dd|dl|html)[^>]*>").unwrap();
    let re_comments = Regex::new(r"<!--[\w\W\r\n]*?-->").unwrap();
    let mut result = re_tags.replace_all(html, "\n").to_string();
    result = re_comments.replace_all(&result, "").to_string();
    html_decode(&result)
}

pub struct HtmlAnalyzer {
    content: String,
}

impl Analyzer for HtmlAnalyzer {
    fn parse(content: &str) -> crate::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            content: content.to_string(),
        })
    }

    fn get_elements(&self, rule: &str) -> crate::Result<Vec<String>> {
        let document = Html::parse_document(&self.content);
        let selector = Selector::parse(rule.trim()).map_err(|e| anyhow!("{e}"))?;

        Ok(document.select(&selector).map(|el| el.html()).collect())
    }

    fn get_string(&self, rule: &str) -> crate::Result<String> {
        Ok(self.get_string_list(rule)?.join("  "))
    }

    fn get_string_list(&self, rule: &str) -> crate::Result<Vec<String>> {
        if !rule.contains('@') {
            return Ok(vec![self._get_result(rule, None)]);
        }

        let (selectors, last_rule) = rule.split_once('@').unwrap();
        let document = Html::parse_document(&self.content);

        if selectors.is_empty() {
            return Ok(vec![]);
        }
        let selector = Selector::parse(selectors).expect("Invalid selector");

        Ok(document
            .select(&selector)
            .map(|el| self._get_result(last_rule, Some(el.html().as_str())))
            .collect())
    }
}

impl HtmlAnalyzer {
    fn _get_result(&self, last_rule: &str, html: Option<&str>) -> String {
        let document = Html::parse_fragment(html.unwrap_or(&self.content));

        match last_rule {
            "text" => document.root_element().text().collect::<String>(),
            "textNodes" => {
                let selector = Selector::parse(":root > *").unwrap();
                document
                    .select(&selector)
                    .map(|el| el.text().collect::<String>())
                    .collect::<Vec<String>>()
                    .join("\n")
                    .trim()
                    .to_string()
            }
            "outerHtml" => document.html(),
            "innerHtml" => {
                let selector = Selector::parse(":root").unwrap();
                document
                    .select(&selector)
                    .map(|el| el.inner_html())
                    .collect::<Vec<String>>()
                    .join("\n")
                    .trim()
                    .to_string()
            }
            "html" => get_html_string(document.html().as_str()),
            _ => document
                .root_element()
                .child_elements()
                .next()
                .unwrap()
                .attr(last_rule)
                .unwrap_or("")
                .to_string(),
        }
    }
}

#[cfg(test)]
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_html_analyzer() {
        let analyzer = HtmlAnalyzer::parse(include_str!("../../test-html/4.html")).unwrap();
        let res = analyzer
            .get_string(".result-game-item-info p:nth-of-type(1) span:nth-of-type(2) @text")
            .unwrap();
        println!("{:#?}", res);
        println!("{:#?}", res.len());
    }
}
