use crate::{
    utils::{replace_all, value_to_string},
    Result,
};
use anyhow::anyhow;
use regex::Regex;
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Variables {
    pub variables: HashMap<String, String>,
    pub put_rule: Regex,
    pub get_rule: Regex,
}

impl Variables {
    pub fn new() -> Result<Self> {
        Ok(Self {
            variables: HashMap::new(),
            put_rule: Regex::new(r"@put:\{(\w+):(\$.+)\}")?,
            get_rule: Regex::new(r"@get:\{(\w+)\}")?,
        })
    }

    pub fn put(&mut self, json_rule: &str, data: &Value) -> Result<String> {
        replace_all(&self.put_rule, json_rule, |capture| {
            let key = capture.get(1).ok_or(anyhow!("key is not found"))?.as_str();
            let path = capture
                .get(2)
                .ok_or(anyhow!("json path is not found"))?
                .as_str();
            let v = value_to_string(data, path)?;
            self.variables.insert(key.to_string(), v);
            Ok("".into())
        })
    }

    pub fn get(&self, json_rule: &str) -> Result<String> {
        replace_all(&self.get_rule, json_rule, |capture| {
            let key = capture.get(1).ok_or(anyhow!("key is not found"))?.as_str();
            let v = self
                .variables
                .get(key)
                .ok_or(anyhow!("key {} is not found", key))?;

            Ok(v.to_string())
        })
    }

    pub fn insert<T: ToString>(&mut self, key: &str, value: T) {
        self.variables.insert(key.to_string(), value.to_string());
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_variables() -> Result<()> {
        let mut variables = Variables::new()?;
        let data = json!({
            "book":{
                "id":"123"
            }
        });
        let rule = "hello world @put:{book:$.book.id}";
        let res = variables.put(rule, &data)?;

        assert_eq!(res, "hello world ");
        assert_eq!(variables.variables.get("book"), Some(&"123".to_string()));

        let rule = "hello world @get:{book}";
        let res = variables.get(rule)?;
        assert_eq!(res, "hello world 123");
        Ok(())
    }
}
