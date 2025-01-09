use std::{collections::HashMap, sync::LazyLock};

use super::{json::value_to_string, Analyzer, AnalyzerType, Analyzers, SingleRule};
use crate::{utils::replace_all, Result};
use anyhow::anyhow;
use regex::Regex;
use serde_json::Value;

static SPLIT_RULE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"@css:|@json:|@http:|@xpath:|@match:|@regex:|@regexp:|@replace:|@encode:|@decode:|^",
    )
    .unwrap()
});
static EXPRESSION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{(.+?)\}\}").unwrap());
static PUT_RULE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"@put:\{(.+?):(.+?)\}").unwrap());
static GET_RULE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"@get:\{(.+?)\}").unwrap());

#[derive(Debug, Clone)]
pub struct AnalyzerManager {
    pub analyzers: Vec<Analyzers>,
    pub variables: HashMap<String, String>,
}

impl AnalyzerManager {
    pub fn new() -> Result<Self> {
        Ok(Self {
            analyzers: vec![
                Analyzers::new(r"^@css:", None, AnalyzerType::Html)?,
                Analyzers::new(r"^@json:|^\$", Some(r"^@json:"), AnalyzerType::JsonPath)?,
                Analyzers::new("", None, AnalyzerType::Default)?,
            ],
            variables: HashMap::new(),
        })
    }

    pub fn set<T: ToString>(&mut self, key: &str, value: T) {
        self.variables.insert(key.to_string(), value.to_string());
    }

    pub fn get_analyzer(&self, rule: &str) -> &Analyzers {
        self.analyzers
            .iter()
            .find(|a| a.pattern.is_match(rule.trim()))
            .unwrap()
    }

    pub fn split_rule_resolve(&self, rule: &str) -> Result<Vec<SingleRule>> {
        let rule_match = SPLIT_RULE.find_iter(rule).collect::<Vec<_>>();
        let mut rule_list: Vec<SingleRule> = vec![];
        let mut end = rule.len();

        for i in rule_match.iter().rev() {
            let mut r = rule[i.start()..end].to_string();
            end = i.start();

            let analyzer = self.get_analyzer(&r);

            r = analyzer
                .replace
                .as_ref()
                .unwrap_or(&analyzer.pattern)
                .replace(&r, "")
                .to_string();

            if let Some(index) = r.find("##") {
                // 按## 分割
                let (r, replace) = r.split_at(index);

                rule_list.push(SingleRule::new(
                    r,
                    // 去掉 ##
                    Some(&replace[2..]),
                    analyzer.analyzer.clone(),
                )?);
            } else {
                rule_list.push(SingleRule::new(&r, None, analyzer.analyzer.clone())?);
            }
        }

        rule_list.reverse();
        Ok(rule_list)
    }

    fn _get_elements(analyzer: &dyn Analyzer, rule: &str) -> Result<Vec<String>> {
        if rule.contains("&&") {
            let mut res = vec![];
            for simple_rule in rule.split("&&") {
                let mut r = Self::_get_elements(analyzer, simple_rule)?;

                if !r.is_empty() {
                    res.append(&mut r);
                }
            }
            return Ok(res);
        } else if rule.contains("||") {
            for simple_rule in rule.split("||") {
                let r = Self::_get_elements(analyzer, simple_rule)?;

                if !r.is_empty() {
                    return Ok(r);
                };
            }
        }
        analyzer.get_elements(rule)
    }

    pub fn get_element(&self, rule: &str, data: &str) -> Result<Vec<String>> {
        let mut temp = data.to_string();

        for single_rule in self.split_rule_resolve(rule)? {
            let analyzer = single_rule.analyzer.parse_to_analyzer(&temp)?;
            temp = Self::_get_elements(analyzer.as_ref(), &single_rule.rule)?
                .join("_______split_______");
        }

        Ok(temp
            .split("_______split_______")
            .map(|s| s.to_string())
            .collect())
    }

    fn _get_string(
        single_rule: &SingleRule,
        analyzer: &dyn Analyzer,
        rule: &str,
    ) -> Result<String> {
        let mut result = String::new();

        if rule.contains("&&") {
            let mut res = vec![];
            for simple_rule in rule.split("&&") {
                let r = Self::_get_string(single_rule, analyzer, simple_rule)?;

                if !r.is_empty() {
                    res.push(r);
                }
            }

            return Ok(res.join("  "));
        } else if rule.contains("||") {
            for simple_rule in rule.split("||") {
                let r = Self::_get_string(single_rule, analyzer, simple_rule)?;

                if !r.is_empty() {
                    return Ok(r);
                };
            }
        } else {
            result = analyzer.get_string(rule)?.trim().to_string()
        }

        if result.is_empty() {
            Ok(result)
        } else {
            Ok(single_rule.replace_content(&result)?)
        }
    }

    fn put_variable(&mut self, rule: &str, data: &str) -> Result<String> {
        replace_all(&PUT_RULE, rule, |capture| {
            let key = capture
                .get(1)
                .ok_or(anyhow!("key is not found"))?
                .as_str()
                .trim();

            let sub_rule = capture
                .get(2)
                .ok_or(anyhow!("value rule is not found"))?
                .as_str()
                .trim();

            let v = self.get_string(sub_rule, data, None)?;
            self.variables.insert(key.to_string(), v);
            Ok("".into())
        })
    }

    fn get_variable(&self, rule: &str) -> Result<String> {
        replace_all(&GET_RULE, rule, |capture| {
            let key = capture
                .get(1)
                .ok_or(anyhow!("key is not found"))?
                .as_str()
                .trim();

            let v = self
                .variables
                .get(key)
                .ok_or(anyhow!("the value of key {} is not found", key))?;

            Ok(v.to_string())
        })
    }

    pub fn get_string(&mut self, rule: &str, data: &str, extra: Option<Value>) -> Result<String> {
        if rule.is_empty() {
            return Ok("".to_string());
        }

        // 处理put
        let new_rule = self.put_variable(rule, data)?;

        // 处理get
        let new_rule = self.get_variable(&new_rule)?;

        // 处理表达式
        let p_left = new_rule.rfind("{{");
        let p_right = new_rule.rfind("}}");

        if p_left.is_some() && p_right.is_some() {
            let left = p_left.unwrap();
            let right = p_right.unwrap();

            if left < right {
                return replace_all(&EXPRESSION, &new_rule, |captures| {
                    let sub_rule = captures.get(1).map(|m| m.as_str().trim()).unwrap_or("");
                    if extra.is_some() {
                        if let Some(extra_value) = extra.as_ref().unwrap().get(sub_rule) {
                            return value_to_string(extra_value);
                        }
                    }
                    self.get_string(sub_rule, data, None)
                });
            }
        }

        // 处理普通规则
        let mut temp = data.to_string();
        for single_rule in self.split_rule_resolve(&new_rule)? {
            let analyzer = single_rule.analyzer.parse_to_analyzer(&temp)?;

            temp = Self::_get_string(&single_rule, analyzer.as_ref(), &single_rule.rule)?;
            temp = single_rule.replace_content(&temp)?;
        }
        Ok(temp)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_analyzer_manager() {
        let mut analyzer_manager = AnalyzerManager::new().unwrap();
        let data = "{\"buymessagevalue\":\"15_15\",\"chapter_id\":300,\"chapter_name\":\"第四卷 剑气近_第二百九十五章 远望\",\"chapter_size\":3253,\"coin\":15,\"coin_original\":15,\"createdate\":\"2023-04-27 23:19:25\",\"license\":1,\"money\":0.15,\"novel_bkid_crid\":\"novel_672340121_300\",\"ori_license\":1,\"txt_url\":\"\",\"zip_url\":\"\"}";
        analyzer_manager.set("book", 123);
        analyzer_manager.set("index", 1);

        let res = analyzer_manager.get_string(
            "https://www.xmkanshu.com/service/getContent?fr=smsstg&v=4&uid=B197589CF54DC527538FADCAE6BDBC78&urbid=%2Fbook_95_0&bkid=@get:{book}&crid={{$.chapter_id}}&pg=1",
            data,
            Some(json!({
                "page":123
            })),
        );
        assert_eq!(res.unwrap(), "https://www.xmkanshu.com/service/getContent?fr=smsstg&v=4&uid=B197589CF54DC527538FADCAE6BDBC78&urbid=%2Fbook_95_0&bkid=123&crid=300&pg=1");
    }

    #[test]
    fn test_analyzer_manager_get_analyzer() {
        let analyzer_manager = AnalyzerManager::new().unwrap();
        let analyzer =
            analyzer_manager.get_analyzer("[property=og:novel:latest_chapter_name]@content");
        assert_eq!(analyzer.analyzer, AnalyzerType::Default);

        let analyzer = analyzer_manager.get_analyzer("$.book.id##4##abc");
        assert_eq!(analyzer.analyzer, AnalyzerType::JsonPath);

        let analyzer = analyzer_manager.get_analyzer("@css:div h1 a[href]");
        assert_eq!(analyzer.analyzer, AnalyzerType::Html);
    }
}
