use crate::Result;
use jsonpath_rust::JsonPath;
use regex::{Captures, Regex};
use serde_json::Value;
use std::str::FromStr;

pub mod json_data;
pub mod parse_url;

pub use json_data::*;
pub use parse_url::*;

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

pub fn split_preserving_delimiters(text: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current_segment = String::new();
    let mut inside_double_braces = false;
    let mut chars = text.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' && chars.peek() == Some(&'{') {
            // 开始 {{ }}
            inside_double_braces = true;
            current_segment.push(c);
            current_segment.push(chars.next().unwrap()); // 吞掉第二个 {
        } else if c == '}' && chars.peek() == Some(&'}') {
            // 结束 {{ }}
            inside_double_braces = false;
            current_segment.push(c);
            current_segment.push(chars.next().unwrap()); // 吞掉第二个 }
        } else if !inside_double_braces && c == '#' && chars.peek() == Some(&'#') {
            // 遇到 ## 并且不在 {{ }} 内部
            result.push(current_segment.trim().to_string());
            current_segment = String::new();
            chars.next();
        } else {
            // 普通字符，直接添加到当前段落
            current_segment.push(c);
        }
    }

    // 添加最后一个段落（如果有）
    if !current_segment.is_empty() {
        result.push(current_segment.trim().to_string());
    }

    result
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::Variables;
    use serde_json::json;

    /// 测试包含key和
    #[test]
    fn test_parse_url() -> Result<()> {
        let s = "The rain in Spain {{key}} {{$.hello}} {{page}} stays mainly in the plain";
        let res = parse_url(
            s,
            &Params {
                key: Some("hello".to_string()),
                page: Some(1),
                page_size: None,
            },
            Some(&json!({
                "hello": "world"
            })),
        )?;
        assert_eq!(
            "The rain in Spain hello world 1 stays mainly in the plain",
            res
        );
        Ok(())
    }

    #[test]
    fn test_data_with_regex() -> Result<()> {
        let mut variables = Variables::new()?;
        let res = JsonData::try_from("{{$.result.book_category_full_name##;##,}},{{$.result.book_score}}分,{{$.result.last_update_time##\\s.*}}")?;
        assert!(res.regex.is_none());
        let result = json!({
            "book_size": "123456",
            "name":"abc",
            "result":{
                "book_category_full_name":"奇幻玄幻;玄幻",
                "book_score":10,
                "last_update_time":"2024-08-17 22:14:42"
            }
        });
        assert_eq!(
            "奇幻玄幻,玄幻,10分,2024-08-17",
            res.parse_data(&result, &mut variables)?
        );
        Ok(())
    }
}
