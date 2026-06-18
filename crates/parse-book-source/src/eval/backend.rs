//! 抽取后端(Strategy)。按 `Via` 静态分派到 css(dom_query)/ json(jsonpath)/ regex /
//! raw 实现;新增 xpath 只需加一个分支(开闭原则,见 design D8)。
//!
//! HTML 的 `select` 为 **self-or-descendant** 语义:把上下文当文档解析,选择器既匹配
//! 后代、也匹配根元素自身(dom_query 解析片段后根入树),这与旧引擎一致,使
//! `select:"a" + attr:href` 能取「列表项自身的 href」、`select:"h2"` 能判「该项是不是卷」。

use crate::error::EvalError;
use crate::source::{Extract, ExtractOp, Via};
use dom_query::{Document, Matcher};
use fancy_regex::Regex;
use jsonpath_rust::JsonPath;
use serde_json::Value;
use std::sync::LazyLock;

/// 从上下文抽取一个值(值规则)。
pub fn extract(
    via: Via,
    content: &str,
    select: Option<&str>,
    index: Option<i64>,
    ex: &Extract,
) -> Result<String, EvalError> {
    match via {
        Via::Css => html_extract(content, select, index, ex),
        Via::Json => json_extract(content, select, index, ex),
        Via::Regex => regex_extract(content, select, index),
        Via::Raw => Ok(content.to_string()),
        Via::Xpath => super::xpath::xpath_extract(content, select, index, ex),
    }
}

/// 选中所有匹配,返回各自的「子上下文」内容串(列表规则)。
pub fn select_all(via: Via, content: &str, select: &str) -> Result<Vec<String>, EvalError> {
    match via {
        Via::Css => {
            let doc = Document::from(content.to_string());
            let matcher =
                Matcher::new(select).map_err(|_| EvalError::Selector(select.to_string()))?;
            let sel = doc.select_matcher(&matcher);
            Ok(sel.nodes().iter().map(|n| n.html().to_string()).collect())
        }
        Via::Json => {
            let value: Value = parse_json_content(content)?;
            let matched = value
                .query(select)
                .map_err(|e| EvalError::JsonPath(e.to_string()))?;
            // value_to_string:字符串值取内容(不带 JSON 引号),供下游 item 规则求值。
            Ok(matched.into_iter().map(value_to_string).collect())
        }
        Via::Regex => {
            let re = Regex::new(select).map_err(|e| EvalError::Regex(e.to_string()))?;
            Ok(re
                .find_iter(content)
                .filter_map(|m| m.ok())
                .map(|m| m.as_str().to_string())
                .collect())
        }
        Via::Raw => Ok(vec![content.to_string()]),
        Via::Xpath => super::xpath::xpath_select_all(content, select),
    }
}

// ───────────────────────── HTML(dom_query)─────────────────────────

fn html_extract(
    content: &str,
    select: Option<&str>,
    index: Option<i64>,
    ex: &Extract,
) -> Result<String, EvalError> {
    let doc = Document::from(content.to_string());
    // 省略 select 时作用于整篇(根);否则按选择器(self-or-descendant)。
    // 用 Matcher 区分「选择器非法」(报错)与「合法但无匹配」(返回空)。
    let sel = match select {
        Some(s) => {
            let matcher = Matcher::new(s).map_err(|_| EvalError::Selector(s.to_string()))?;
            doc.select_matcher(&matcher)
        }
        None => doc.select(":root"),
    };
    let nodes = sel.nodes();
    if nodes.is_empty() {
        return Ok(String::new());
    }
    let node = &nodes[resolve_index(index, nodes.len())];
    Ok(match ex {
        // 文本/属性默认去首尾空白(标题/链接等场景几乎总是期望的;与旧引擎一致)。
        Extract::Op(ExtractOp::Text) => node.text().trim().to_string(),
        Extract::Op(ExtractOp::OwnText) => node.immediate_text().trim().to_string(),
        // HTML 正文保留结构,清洗交给 `clean` 步骤。
        Extract::Op(ExtractOp::Html) => clean_html(&node.inner_html()),
        Extract::Op(ExtractOp::InnerHtml) => node.inner_html().to_string(),
        Extract::Op(ExtractOp::OuterHtml) => node.html().to_string(),
        Extract::Attr { attr } => node
            .attr(attr)
            .map(|s| s.trim().to_string())
            .unwrap_or_default(),
    })
}

/// 把正文 HTML 转为可读文本:块级/换行标签 → 换行,去注释,解码常见实体。
/// (对应旧引擎的 `get_html_string`,用于 `extract: "html"`;xpath 后端复用。)
pub(crate) fn clean_html(html: &str) -> String {
    // 以下均为编译期写死的合法正则,运行期不可能编译失败,故 unwrap 安全。
    static TAGS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"</?(?:div|p|br|hr|h[1-6]|article|section|dd|dl|li)[^>]*>").unwrap()
    });
    static COMMENTS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<!--[\s\S]*?-->").unwrap());
    static OTHER_TAGS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^>]+>").unwrap());

    let s = TAGS.replace_all(html, "\n");
    let s = COMMENTS.replace_all(&s, "");
    let s = OTHER_TAGS.replace_all(&s, "");
    decode_entities(&s)
}

fn decode_entities(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&nbsp;", " ")
        .replace("&#39;", "'")
        .replace("&quot;", "\"")
}

// ───────────────────────── JSON(jsonpath-rust 1.x)─────────────────────────

/// 把上下文解析为 JSON。**空响应**(常见于被反爬拦截 / 网络故障 / 触发风控时返回空体)
/// 给出明确提示,而非把 serde 的「EOF while parsing a value at line 1 column 0」原样透出,
/// 让用户能区分「站点没返回内容」与「规则取错了字段」。
fn parse_json_content(content: &str) -> Result<Value, EvalError> {
    if content.trim().is_empty() {
        return Err(EvalError::Json(
            "响应体为空(疑似被反爬拦截或网络故障)".to_string(),
        ));
    }
    serde_json::from_str(content).map_err(|e| EvalError::Json(e.to_string()))
}

fn json_extract(
    content: &str,
    select: Option<&str>,
    index: Option<i64>,
    ex: &Extract,
) -> Result<String, EvalError> {
    let value: Value = parse_json_content(content)?;
    let path = select.unwrap_or("$");
    let matched = value
        .query(path)
        .map_err(|e| EvalError::JsonPath(e.to_string()))?;
    if matched.is_empty() {
        return Ok(String::new());
    }
    let v = matched[resolve_index(index, matched.len())];
    // JSON 上下文里 attr 无意义,统一取标量字符串。
    let _ = ex;
    Ok(value_to_string(v))
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

// ───────────────────────── Regex ─────────────────────────

fn regex_extract(
    content: &str,
    select: Option<&str>,
    index: Option<i64>,
) -> Result<String, EvalError> {
    let pat = select.unwrap_or("");
    let re = Regex::new(pat).map_err(|e| EvalError::Regex(e.to_string()))?;
    let caps: Vec<String> = re
        .captures_iter(content)
        .filter_map(|c| c.ok())
        .map(|c| {
            // 有捕获组取第 1 组,否则取整体匹配
            c.get(1)
                .or_else(|| c.get(0))
                .map(|m| m.as_str().to_string())
                .unwrap_or_default()
        })
        .collect();
    if caps.is_empty() {
        return Ok(String::new());
    }
    Ok(caps[resolve_index(index, caps.len())].clone())
}

// ───────────────────────── 公共 ─────────────────────────

/// 解析索引:None→0;负数从末尾;越界回退到首/末。
pub(crate) fn resolve_index(index: Option<i64>, len: usize) -> usize {
    match index {
        None => 0,
        Some(i) if i >= 0 => (i as usize).min(len - 1),
        Some(i) => {
            let from_end = (-i) as usize;
            len.saturating_sub(from_end)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::ExtractOp;

    #[test]
    fn empty_json_response_gives_friendly_error() {
        // 回归:空响应(被反爬拦截/网络故障返回空体)应给「响应体为空」提示,
        // 而非透出 serde 的 "EOF while parsing a value at line 1 column 0"。
        for content in ["", "   ", "\n\t "] {
            let msg = select_all(Via::Json, content, "$.a")
                .unwrap_err()
                .to_string();
            assert!(msg.contains("响应体为空"), "应提示响应为空,实际: {msg}");
            assert!(!msg.contains("EOF"), "不应透出 serde EOF: {msg}");
        }
        // extract 路径同样覆盖。
        let ex = Extract::Op(ExtractOp::Text);
        let msg = extract(Via::Json, "", Some("$.a"), None, &ex)
            .unwrap_err()
            .to_string();
        assert!(
            msg.contains("响应体为空"),
            "extract 空响应也应友好提示: {msg}"
        );
    }

    #[test]
    fn nonempty_invalid_json_still_errors_normally() {
        // 非空但非法 JSON 仍按原样报错,不被误判为「空响应」。
        let msg = select_all(Via::Json, "{not json", "$.a")
            .unwrap_err()
            .to_string();
        assert!(
            !msg.contains("响应体为空"),
            "非空非法 JSON 不应判为空: {msg}"
        );
    }
}
