use crate::Result;
use regex::Regex;
use std::fmt::Debug;
pub mod analyzer_manager;
pub mod default;
pub mod html;
pub mod json;
pub use analyzer_manager::AnalyzerManager;
pub use default::DefaultAnalyzer;
pub use html::HtmlAnalyzer;
pub use json::JsonPathAnalyzer;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnalyzerType {
    JsonPath,
    Html,
    Default,
}

impl AnalyzerType {
    pub fn parse_to_analyzer(&self, date: &str) -> Result<Box<dyn Analyzer>> {
        match self {
            AnalyzerType::JsonPath => Ok(Box::new(JsonPathAnalyzer::parse(date)?)),
            AnalyzerType::Html => Ok(Box::new(HtmlAnalyzer::parse(date)?)),
            AnalyzerType::Default => Ok(Box::new(DefaultAnalyzer::parse(date)?)),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Analyzers {
    pub pattern: Regex,
    pub replace: Option<Regex>,
    pub analyzer: AnalyzerType,
}

impl Analyzers {
    pub fn new(pattern: &str, replace: Option<&str>, analyzer: AnalyzerType) -> Result<Self> {
        Ok(Self {
            pattern: Regex::new(pattern)?,
            replace: replace.map(Regex::new).transpose()?,
            analyzer,
        })
    }
}

#[derive(Debug, Clone)]
pub struct SingleRule {
    pub rule: String,
    // 替换内容(## 后面的内容)
    pub replace: String,
    pub analyzer: AnalyzerType,
}

impl SingleRule {
    pub fn new(rule: &str, replace: Option<&str>, analyzer: AnalyzerType) -> Result<Self> {
        Ok(Self {
            rule: rule.to_string(),
            replace: replace.unwrap_or("").to_string(),
            analyzer,
        })
    }

    pub fn replace_content(&self, content: &str) -> Result<String> {
        if self.replace.is_empty() {
            return Ok(content.to_string());
        }

        if let Some((regex, replace_content)) = self.replace.split_once("##") {
            let regex = Regex::new(regex)?;
            Ok(regex.replace_all(content, replace_content).to_string())
        } else {
            let regex = Regex::new(&self.replace)?;
            Ok(regex.replace_all(content, "").to_string())
        }
    }
}

pub trait Analyzer {
    fn parse(content: &str) -> Result<Self>
    where
        Self: Sized;

    fn get_string(&self, rule: &str) -> Result<String> {
        let _ = rule;
        unimplemented!()
    }

    fn get_string_list(&self, rule: &str) -> Result<Vec<String>> {
        let _ = rule;
        unimplemented!()
    }

    fn get_elements(&self, rule: &str) -> Result<Vec<String>> {
        let _ = rule;
        unimplemented!()
    }
}
