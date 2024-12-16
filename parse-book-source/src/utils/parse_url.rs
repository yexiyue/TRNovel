use crate::{
    utils::{json_path, replace_all, JsonData},
    Result, Variables,
};
use anyhow::anyhow;
use regex::Regex;
use serde_json::Value;

#[derive(Debug, Clone, Default)]
pub struct Params {
    pub key: Option<String>,
    pub page: Option<usize>,
    pub page_size: Option<usize>,
}

impl Params {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn key(mut self, key: &str) -> Self {
        self.key = Some(key.into());
        self
    }

    pub fn page(mut self, page: usize) -> Self {
        self.page = Some(page);
        self
    }

    pub fn page_size(mut self, page_size: usize) -> Self {
        self.page_size = Some(page_size);
        self
    }
}

pub fn parse_url(url: &str, params: &Params, data: Option<&Value>) -> Result<String> {
    let regex = Regex::new(r"\{\{(.*?)\}\}")?;

    replace_all(&regex, url, |captures| {
        let key = captures.get(1).ok_or(anyhow!("key is not found"))?;
        match key.as_str() {
            "key" => {
                let key = params.key.clone().ok_or(anyhow!("key is not found"))?;
                Ok(key)
            }
            "page" => {
                let page = params.page.ok_or(anyhow!("page is not found"))?;
                Ok(page.to_string())
            }
            "pageSize" => {
                let page_size = params.page_size.ok_or(anyhow!("page_size is not found"))?;
                Ok(page_size.to_string())
            }
            path => {
                let Some(data) = data else {
                    return Err(anyhow!("data is not found").into());
                };

                value_to_string(data, path)
            }
        }
    })
}

pub fn parse_template(template: &str, data: &Value, variables: &mut Variables) -> Result<String> {
    let regex = Regex::new(r"\{\{(.*?)\}\}")?;

    if template.contains("{{") {
        replace_all(&regex, template, |captures| {
            let key = captures.get(1).ok_or(anyhow!("key is not found"))?;

            let path = key.as_str();
            let data_with_regex = JsonData::try_from(path)?;
            data_with_regex.parse_data(data, variables)
        })
    } else {
        match value_to_string(data, template) {
            Ok(s) => Ok(s),
            Err(_) => Ok(template.to_string()),
        }
    }
}

pub fn value_to_string(data: &Value, path: &str) -> Result<String> {
    let value = json_path(data, path)?;
    match value
        .get(0)
        .ok_or(anyhow!("the value of {} is not found", path))?
    {
        Value::String(s) => Ok(s.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        _ => Err(anyhow!("the value of {} is not string or number", path).into()),
    }
}
