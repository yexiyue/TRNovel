//! XPath 抽取后端(纯 Rust,零 C 依赖)。
//!
//! 书源 HTML 常不良构(未闭合标签、属性无引号、HTML5 void 元素),而 `sxd-xpath`
//! 基于 XML-strict 的 `sxd-document`。本模块用「宽松进、规整出」的桥:复用已有的
//! `dom_query`(html5ever)宽松解析 DOM,再**遍历建树**直接构造 `sxd` 文档(免去
//! 序列化→重解析的 void 元素脆弱性),最后用 XPath 1.0 求值。
//!
//! 求值语义对齐 css/json 后端(见 `backend`):
//! - 标量结果(`//a/@href`、`string(...)`、`concat(...)`)直接取值,`extract` 忽略;
//! - 元素节点集按 `index` 取节点后套用 `extract`(text/ownText/html/innerHtml/outerHtml/attr);
//! - 列表规则(`select_all`)返回每个匹配元素的 outerXML 子上下文,供后续 item 规则续求值。
//!
//! 失败安全降级:HTML 桥接为空 / XPath 合法但无匹配 → 空结果(交 `firstOf` 兜底);
//! 仅 XPath 表达式本身语法非法 → `EvalError::Xpath`。

use super::backend::{clean_html, resolve_index};
use super::error::EvalError;
use super::source::{Extract, ExtractOp};
use sxd_document::Package;
use sxd_document::dom::{ChildOfElement, Document as SxdDoc, Element as SxdEl};
use sxd_xpath::nodeset::Node as XNode;
use sxd_xpath::{Context, Factory, Value};

/// 抽取一个值(值规则)。
pub fn xpath_extract(
    content: &str,
    select: Option<&str>,
    index: Option<i64>,
    ex: &Extract,
) -> Result<String, EvalError> {
    let Some(select) = select else {
        // 无表达式:XPath 无意义,退化为空(列表场景由 select_all 处理上下文自身)。
        return Ok(String::new());
    };
    let pkg = build_package(content);
    let doc = pkg.as_document();
    let Some(value) = eval(&doc, select)? else {
        return Ok(String::new());
    };
    Ok(match value {
        Value::Boolean(b) => b.to_string(),
        Value::Number(n) => fmt_number(n),
        Value::String(s) => s.trim().to_string(),
        Value::Nodeset(ns) => {
            let nodes = ns.document_order();
            if nodes.is_empty() {
                return Ok(String::new());
            }
            extract_from_node(nodes[resolve_index(index, nodes.len())], ex)
        }
    })
}

/// 选中所有匹配,返回各自的「子上下文」串(列表规则)。
pub fn xpath_select_all(content: &str, select: &str) -> Result<Vec<String>, EvalError> {
    let pkg = build_package(content);
    let doc = pkg.as_document();
    let Some(value) = eval(&doc, select)? else {
        return Ok(Vec::new());
    };
    Ok(match value {
        // 元素 → outerXML 子上下文;属性/文本 → 其字符串值。
        Value::Nodeset(ns) => ns
            .document_order()
            .into_iter()
            .map(|n| match n {
                XNode::Element(e) => element_outer_xml(e),
                XNode::Attribute(a) => a.value().to_string(),
                XNode::Text(t) => t.text().to_string(),
                _ => String::new(),
            })
            .filter(|s| !s.is_empty())
            .collect(),
        // 标量作为列表退化为单值(布尔无列表语义,丢弃)。
        Value::String(s) if !s.is_empty() => vec![s],
        Value::Number(n) => vec![fmt_number(n)],
        _ => Vec::new(),
    })
}

// ───────────────────────── XPath 求值 ─────────────────────────

/// 编译并求值 XPath;表达式空 → `Ok(None)`;语法非法/执行错 → `Err(Xpath)`。
fn eval<'d>(doc: &SxdDoc<'d>, select: &str) -> Result<Option<Value<'d>>, EvalError> {
    let factory = Factory::new();
    let Some(xpath) = factory
        .build(select)
        .map_err(|e| EvalError::Xpath(e.to_string()))?
    else {
        return Ok(None);
    };
    let ctx = Context::new();
    let value = xpath
        .evaluate(&ctx, XNode::Root(doc.root()))
        .map_err(|e| EvalError::Xpath(e.to_string()))?;
    Ok(Some(value))
}

/// XPath number → 字符串:整数不带 `.0`(如 `count()`=5 → "5")。
fn fmt_number(n: f64) -> String {
    if n.is_finite() && n.fract() == 0.0 {
        (n as i64).to_string()
    } else {
        n.to_string()
    }
}

/// 按 `Extract` 从一个 XPath 节点取值。
fn extract_from_node(node: XNode, ex: &Extract) -> String {
    match node {
        XNode::Element(e) => match ex {
            Extract::Op(ExtractOp::Text) => element_text(e).trim().to_string(),
            Extract::Op(ExtractOp::OwnText) => element_own_text(e).trim().to_string(),
            Extract::Op(ExtractOp::Html) => clean_html(&element_inner_xml(e)),
            Extract::Op(ExtractOp::InnerHtml) => element_inner_xml(e),
            Extract::Op(ExtractOp::OuterHtml) => element_outer_xml(e),
            Extract::Attr { attr } => e
                .attribute_value(attr.as_str())
                .map(|s| s.trim().to_string())
                .unwrap_or_default(),
        },
        XNode::Attribute(a) => a.value().trim().to_string(),
        XNode::Text(t) => t.text().trim().to_string(),
        _ => String::new(),
    }
}

// ───────────────────────── 宽松 HTML → sxd 文档(遍历建树)─────────────────────────

/// 用 dom_query 宽松解析 HTML,再遍历建 sxd 文档(html5ever 已补全/规整标签嵌套)。
fn build_package(content: &str) -> Package {
    let dq = dom_query::Document::from(content.to_string());
    let pkg = Package::new();
    {
        let sdoc = pkg.as_document();
        let root = sdoc.root();
        // 顶层只接元素子(跳过 doctype/注释);html5ever 保证有单一 <html> 根,
        // 既满足 XML 单根、又保留 `/html/body/...` 绝对路径可用。
        for child in dq.root().children() {
            if child.is_element() {
                let el = build_element(&child, sdoc);
                root.append_child(el);
            }
        }
    }
    pkg
}

/// 递归把一个 dom_query 元素节点构造为 sxd 元素(含属性与子节点)。
fn build_element<'d>(node: &dom_query::Node, sdoc: SxdDoc<'d>) -> SxdEl<'d> {
    let name = node
        .node_name()
        .map(|n| safe_name(&n))
        .unwrap_or_else(|| "node".to_string());
    let el = sdoc.create_element(name.as_str());
    for attr in node.attrs() {
        let an = safe_name(&attr.name.local);
        if !an.is_empty() {
            el.set_attribute_value(an.as_str(), &attr.value);
        }
    }
    for child in node.children() {
        if child.is_element() {
            el.append_child(build_element(&child, sdoc));
        } else if child.is_text() {
            el.append_child(sdoc.create_text(&child.text()));
        }
    }
    el
}

/// 规整名字为 XML 友好形式:`:` → `_`(避开 sxd 命名空间解析),空 → "node"。
fn safe_name(name: &str) -> String {
    let s = name.trim().replace(':', "_");
    if s.is_empty() { "node".to_string() } else { s }
}

// ───────────────────────── sxd 元素 → 文本 / XML 串 ─────────────────────────

/// 元素的全部后代文本(对应 `extract: text`)。
fn element_text(e: SxdEl) -> String {
    let mut out = String::new();
    collect_text(e, &mut out);
    out
}

fn collect_text(e: SxdEl, out: &mut String) {
    for c in e.children() {
        match c {
            ChildOfElement::Element(ce) => collect_text(ce, out),
            ChildOfElement::Text(t) => out.push_str(t.text()),
            _ => {}
        }
    }
}

/// 元素的直接子文本(对应 `extract: ownText`)。
fn element_own_text(e: SxdEl) -> String {
    let mut out = String::new();
    for c in e.children() {
        if let ChildOfElement::Text(t) = c {
            out.push_str(t.text());
        }
    }
    out
}

/// 元素内层 XML(对应 `extract: innerHtml`;`html` 再经 `clean_html`)。
fn element_inner_xml(e: SxdEl) -> String {
    let mut out = String::new();
    write_children(e, &mut out);
    out
}

/// 元素外层 XML(对应 `extract: outerHtml`;也作 `select_all` 的子上下文)。
fn element_outer_xml(e: SxdEl) -> String {
    let mut out = String::new();
    write_element(e, &mut out);
    out
}

fn write_element(e: SxdEl, out: &mut String) {
    let name = e.name().local_part();
    out.push('<');
    out.push_str(name);
    for a in e.attributes() {
        out.push(' ');
        out.push_str(a.name().local_part());
        out.push_str("=\"");
        escape_into(a.value(), true, out);
        out.push('"');
    }
    out.push('>');
    write_children(e, out);
    out.push_str("</");
    out.push_str(name);
    out.push('>');
}

fn write_children(e: SxdEl, out: &mut String) {
    for c in e.children() {
        match c {
            ChildOfElement::Element(ce) => write_element(ce, out),
            ChildOfElement::Text(t) => escape_into(t.text(), false, out),
            _ => {}
        }
    }
}

/// 转义 XML 文本/属性值(属性额外转义 `"`)。
fn escape_into(s: &str, attr: bool, out: &mut String) {
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' if attr => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Extract;

    const DIRTY: &str = r#"<html><body>
        <div class="title">  书名  </div>
        <ul id="toc">
          <li><a href="/c/1.html">第一章</a></li>
          <li><a href="/c/2.html">第二章<br>未闭合</li>
        </ul>
        <img src="/cover.jpg">
      </body></html>"#;

    fn attr(name: &str) -> Extract {
        Extract::Attr { attr: name.into() }
    }

    #[test]
    fn element_text_with_extract() {
        let v = xpath_extract(
            DIRTY,
            Some("//div[@class='title']"),
            None,
            &Extract::default(),
        )
        .unwrap();
        assert_eq!(v, "书名");
    }

    #[test]
    fn scalar_attribute() {
        // 标量结果(/@href)直接取,忽略 extract。
        let v = xpath_extract(
            DIRTY,
            Some("//ul[@id='toc']/li[1]/a/@href"),
            None,
            &Extract::default(),
        )
        .unwrap();
        assert_eq!(v, "/c/1.html");
    }

    #[test]
    fn element_attr_extract() {
        let v = xpath_extract(DIRTY, Some("//img"), None, &attr("src")).unwrap();
        assert_eq!(v, "/cover.jpg");
    }

    #[test]
    fn index_negative_picks_last() {
        let v = xpath_extract(DIRTY, Some("//ul/li/a"), Some(-1), &Extract::default()).unwrap();
        assert_eq!(v, "第二章未闭合");
    }

    #[test]
    fn select_all_returns_subcontexts() {
        let items = xpath_select_all(DIRTY, "//ul[@id='toc']/li").unwrap();
        assert_eq!(items.len(), 2, "脏 HTML(未闭合 li/br)仍被宽松解析为 2 项");
        // 子上下文可被下游继续求值(取各 li 内的 a 文本)。
        let t0 = xpath_extract(&items[0], Some("//a"), None, &Extract::default()).unwrap();
        assert_eq!(t0, "第一章");
    }

    #[test]
    fn empty_match_is_empty_not_error() {
        let v = xpath_extract(DIRTY, Some("//nonexistent"), None, &Extract::default()).unwrap();
        assert_eq!(v, "");
        let l = xpath_select_all(DIRTY, "//nonexistent").unwrap();
        assert!(l.is_empty());
    }

    #[test]
    fn invalid_xpath_errors() {
        let r = xpath_extract(DIRTY, Some("//["), None, &Extract::default());
        assert!(matches!(r, Err(EvalError::Xpath(_))));
    }

    #[test]
    fn count_scalar_number() {
        let v = xpath_extract(DIRTY, Some("count(//ul/li)"), None, &Extract::default()).unwrap();
        assert_eq!(v, "2");
    }
}
