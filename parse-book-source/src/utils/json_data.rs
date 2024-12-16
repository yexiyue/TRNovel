use crate::{ParseError, Result, Variables};
use anyhow::anyhow;
use regex::Regex;
use serde_json::Value;

use super::{parse_template, split_preserving_delimiters};

#[derive(Debug, Clone, Default)]
pub struct JsonData {
    pub data: String,
    pub regex: Option<Regex>,
    pub replace_content: Option<String>,
}

impl JsonData {
    fn replace_all(&self, haystack: &str) -> Result<String> {
        if let Some(re) = &self.regex {
            Ok(re
                .replace_all(
                    haystack,
                    self.replace_content.as_ref().unwrap_or(&"".into()),
                )
                .into())
        } else {
            Ok(haystack.into())
        }
    }

    pub fn parse_data(&self, data: &Value, variables: &mut Variables) -> Result<String> {
        let value = variables.put(&self.data, data)?;
        let value = variables.get(&value)?;
        let value = parse_template(&value, data, variables)?;
        self.replace_all(&value)
    }
}

impl TryFrom<&str> for JsonData {
    type Error = ParseError;
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        let str = split_preserving_delimiters(value);

        let mut res = JsonData {
            data: str.first().ok_or(anyhow!("data is not found"))?.to_string(),
            ..Default::default()
        };

        if let Some(regex) = str.get(1) {
            res.regex = Some(Regex::new(regex)?);
        }

        if let Some(content) = str.get(2) {
            res.replace_content = Some(content.to_string());
        }

        Ok(res)
    }
}

impl TryFrom<&String> for JsonData {
    type Error = ParseError;
    fn try_from(value: &String) -> std::result::Result<Self, Self::Error> {
        JsonData::try_from(value.as_str())
    }
}
