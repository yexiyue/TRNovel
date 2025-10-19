use super::Analyzer;
use crate::Result;
use crate::utils::json_path;
use serde_json::Value;

pub struct JsonPathAnalyzer {
    content: Value,
}

impl Analyzer for JsonPathAnalyzer {
    fn parse(content: &str) -> crate::Result<Self>
    where
        Self: Sized,
    {
        Ok(Self {
            content: serde_json::from_str(content)?,
        })
    }

    fn get_string(&self, rule: &str) -> Result<String> {
        let result = json_path(&self.content, rule)?;
        if let Value::Array(arr) = result {
            return value_to_string(&arr[0]);
        }
        value_to_string(&result)
    }

    fn get_elements(&self, rule: &str) -> Result<Vec<String>> {
        let result = json_path(&self.content, rule)?;
        if let Value::Array(arr) = result {
            if arr.len() == 1 && arr[0].is_array() {
                Ok(arr[0]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|v| v.to_string())
                    .collect())
            } else {
                Ok(arr.into_iter().map(|v| v.to_string()).collect())
            }
        } else {
            Ok(vec![result.to_string()])
        }
    }
}

pub fn value_to_string(data: &Value) -> crate::Result<String> {
    match data {
        Value::String(s) => Ok(s.to_string()),
        Value::Number(n) => Ok(n.to_string()),
        Value::Bool(b) => Ok(b.to_string()),
        _ => Err(anyhow::anyhow!("Invalid value type").into()),
    }
}
