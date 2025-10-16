use crate::Result;
use jsonpath_rust::JsonPath;
use regex::{Captures, Regex};
use serde_json::Value;
use std::str::FromStr;

pub fn replace_all(
    re: &Regex,
    haystack: &str,
    mut replacement: impl FnMut(&Captures) -> Result<String>,
) -> Result<String> {
    let mut new = String::with_capacity(haystack.len());
    let mut last_match = 0;
    for caps in re.captures_iter(haystack) {
        let m = caps.get(0).unwrap();
        new.push_str(&haystack[last_match..m.start()]);
        new.push_str(&replacement(&caps)?);
        last_match = m.end();
    }
    new.push_str(&haystack[last_match..]);
    Ok(new)
}

pub fn json_path(data: &Value, path: &str) -> Result<Value> {
    Ok(JsonPath::from_str(path)?.find(data))
}
