//! 规则解释器(Interpreter + Composite)。递归遍历 [`Rule`] 求值;无字符串 DSL 解析。
//!
//! 两个入口:
//! - [`eval_value`]:值规则 → 一个字符串。
//! - [`eval_list`]:列表规则 → 多个「子上下文」内容串(每个供后续 item 规则求值)。

use super::backend;
use super::error::EvalError;
use super::source::{CleanStep, Rule};
use super::transform;
use fancy_regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

/// 模板插值变量表(`{{key}}` / `{{page}}` / `{{base}}` / 命名捕获)。
pub type Vars = HashMap<String, String>;

/// 对当前上下文求一个值。
pub fn eval_value(rule: &Rule, ctx: &str, vars: &Vars) -> Result<String, EvalError> {
    match rule {
        Rule::Literal { literal } => Ok(literal.clone()),
        Rule::Template { template } => Ok(interpolate(template, vars)),
        Rule::FirstOf { first_of } => {
            for r in first_of {
                let v = eval_value(r, ctx, vars)?;
                if !v.trim().is_empty() {
                    return Ok(v);
                }
            }
            Ok(String::new())
        }
        Rule::Concat { concat, join } => {
            let mut parts = Vec::new();
            for r in concat {
                let v = eval_value(r, ctx, vars)?;
                if !v.trim().is_empty() {
                    parts.push(v);
                }
            }
            Ok(parts.join(join))
        }
        Rule::Js { js } => run_js(js, ctx, vars),
        Rule::Leaf(l) => {
            let raw = backend::extract(l.via, ctx, l.select.as_deref(), l.index, &l.extract)?;
            apply_clean(raw, &l.clean, vars)
        }
    }
}

/// 执行一段 JS(逃生舱):以 `result` 为当前上下文、注入变量 + `crypto` 助手。
/// 未启用 `js` feature 时返回 `Unsupported("js")`(但书源仍可解析)。
fn run_js(script: &str, result: &str, vars: &Vars) -> Result<String, EvalError> {
    #[cfg(feature = "js")]
    {
        crate::js::eval_js(script, result, vars)
    }
    #[cfg(not(feature = "js"))]
    {
        let _ = (script, result, vars);
        Err(EvalError::Unsupported("js"))
    }
}

/// 选中所有匹配,返回各自的子上下文内容串。
pub fn eval_list(rule: &Rule, ctx: &str) -> Result<Vec<String>, EvalError> {
    match rule {
        Rule::Leaf(l) => match l.select.as_deref() {
            Some(sel) => backend::select_all(l.via, ctx, sel),
            // 无选择器:把当前上下文作为单一项(而非把空串当非法选择器)。
            None => Ok(vec![ctx.to_string()]),
        },
        Rule::FirstOf { first_of } => {
            for r in first_of {
                let v = eval_list(r, ctx)?;
                if !v.is_empty() {
                    return Ok(v);
                }
            }
            Ok(Vec::new())
        }
        // literal/template/concat 作为列表无意义:退化为单值(若非空)。
        other => {
            let v = eval_value(other, ctx, &Vars::new())?;
            Ok(if v.is_empty() { Vec::new() } else { vec![v] })
        }
    }
}

/// 应用清洗流水线。步内固定顺序:
/// `regex→replace → trim → prepend → append → decode → encode → hash → cipher → fontMap → cn`。
/// 编解码/加解密会失败(非法输入、错密钥),故返回 `Result`(显式报错,不静默空)。
fn apply_clean(mut s: String, steps: &[CleanStep], vars: &Vars) -> Result<String, EvalError> {
    for step in steps {
        if let Some(pat) = &step.regex {
            // 非法正则是配置错误,显式报错(与抽取层 regex_extract 及下方 crypto 步一致),不静默跳过。
            let re = Regex::new(pat).map_err(|e| EvalError::Regex(e.to_string()))?;
            let rep = step.replace.as_deref().unwrap_or("");
            s = re.replace_all(&s, rep).into_owned();
        }
        if step.trim.unwrap_or(false) {
            s = s.trim().to_string();
        }
        if let Some(p) = &step.prepend {
            s = format!("{p}{s}");
        }
        if let Some(a) = &step.append {
            s = format!("{s}{a}");
        }
        if let Some(c) = step.decode {
            s = transform::decode(&s, c)?;
        }
        if let Some(c) = step.encode {
            s = transform::encode(&s, c)?;
        }
        if let Some(h) = &step.hash {
            s = transform::hash(&s, h)?;
        }
        if let Some(c) = &step.cipher {
            s = transform::cipher(&s, c)?;
        }
        if let Some(table) = &step.font_map {
            s = transform::font_map(&s, table)?;
        }
        if let Some(cn) = step.cn {
            s = transform::cn_convert(&s, cn);
        }
        if let Some(js) = &step.js {
            s = run_js(js, &s, vars)?;
        }
    }
    Ok(s)
}

/// 把 `{{key}}` 替换为变量值,未知键替换为空串。
pub(crate) fn interpolate(template: &str, vars: &Vars) -> String {
    static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\{\{\s*([\w.\-]+)\s*\}\}").unwrap());
    RE.replace_all(template, |c: &fancy_regex::Captures| {
        c.get(1)
            .and_then(|m| vars.get(m.as_str()))
            .cloned()
            .unwrap_or_default()
    })
    .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::Rule;

    fn rule(j: &str) -> Rule {
        serde_json::from_str(j).expect("rule json")
    }

    // 合成的 bilixs 式目录:.box 内 直接子 h2(卷)+ a.module-row-text(章);
    // span 包裹的"阅读进度"应被 `.box > h2` 排除。
    const CATALOG: &str = r#"<html><body>
      <div class="box">
        <span id="shuqian"><h2 class="module-title type">阅读进度</h2></span>
        <h2 class="module-title type">第一卷 魔性不改</h2>
        <div class="module-row-info"><a class="module-row-text" href="/n/1.html"><i></i><div class="module-row-title"><span>第一章 甲</span></div></a></div>
        <div class="module-row-info"><a class="module-row-text" href="/n/2.html"><i></i><div class="module-row-title"><span>第二章 乙</span></div></a></div>
        <h2 class="module-title type">第二卷 魔子出山</h2>
        <div class="module-row-info"><a class="module-row-text" href="/n/3.html"><i></i><div class="module-row-title"><span>第三章 丙</span></div></a></div>
      </div>
    </body></html>"#;

    fn toc_list() -> Rule {
        rule(r#"{"via":"css","select":".box > h2.module-title.type, .box a.module-row-text"}"#)
    }

    #[test]
    fn list_selects_volumes_and_chapters_in_document_order() {
        let items = eval_list(&toc_list(), CATALOG).unwrap();
        assert_eq!(items.len(), 5, "2 卷 + 3 章 = 5(排除 span 内的阅读进度)");
    }

    #[test]
    fn toc_rules_split_into_volumes_and_chapters() {
        let name = rule(
            r#"{"firstOf":[{"via":"css","select":".module-row-title","extract":"text"},{"via":"css","select":"h2","extract":"text"}]}"#,
        );
        let url = rule(r#"{"via":"css","select":"a","extract":{"attr":"href"}}"#);
        let is_volume = rule(r#"{"via":"css","select":"h2","extract":"text"}"#);
        let vars = Vars::new();

        let mut chapters = Vec::new();
        let mut volumes = Vec::new();
        for it in eval_list(&toc_list(), CATALOG).unwrap() {
            let nm = eval_value(&name, &it, &vars).unwrap();
            if eval_value(&is_volume, &it, &vars)
                .unwrap()
                .trim()
                .is_empty()
            {
                let u = eval_value(&url, &it, &vars).unwrap();
                chapters.push((nm, u));
            } else {
                volumes.push(nm);
            }
        }
        assert_eq!(volumes, vec!["第一卷 魔性不改", "第二卷 魔子出山"]);
        assert_eq!(chapters.len(), 3);
        assert_eq!(
            chapters[0],
            ("第一章 甲".to_string(), "/n/1.html".to_string())
        );
        assert_eq!(
            chapters[2],
            ("第三章 丙".to_string(), "/n/3.html".to_string())
        );
    }

    #[test]
    fn book_info_extracts_og_meta_attr() {
        let html = r#"<head><meta property="og:novel:book_name" content="蛊真人"><meta property="og:image" content="https://x/c.jpg"></head>"#;
        let name = rule(
            r#"{"via":"css","select":"[property=\"og:novel:book_name\"]","extract":{"attr":"content"}}"#,
        );
        assert_eq!(eval_value(&name, html, &Vars::new()).unwrap(), "蛊真人");
    }

    #[test]
    fn content_html_extract_cleans_paragraphs() {
        let html = r#"<div class="article-content"><p>第一段。</p><p>第二段。</p></div>"#;
        let r = rule(
            r#"{"via":"css","select":".article-content","extract":"html","clean":[{"trim":true}]}"#,
        );
        let out = eval_value(&r, html, &Vars::new()).unwrap();
        assert!(out.contains("第一段。"));
        assert!(out.contains("第二段。"));
        assert!(out.contains('\n'), "段落间应有换行");
    }

    #[test]
    fn clean_font_map_restores_via_inline_table() {
        // camelCase "fontMap" 反序列化 + clean 流水线接线;fontMap 直接是「码点→字」表。
        let r = rule(r#"{"via":"raw","clean":[{"fontMap":{"E001":"甲","E002":"乙"}}]}"#);
        assert_eq!(
            eval_value(&r, "\u{E001}\u{E002}!", &Vars::new()).unwrap(),
            "甲乙!"
        );
    }

    #[test]
    fn template_interpolates_vars() {
        let r = rule(r#"{"template":"{{base}}/search?q={{key}}&pg={{page}}"}"#);
        let mut vars = Vars::new();
        vars.insert("base".into(), "https://x.com".into());
        vars.insert("key".into(), "蛊真人".into());
        vars.insert("page".into(), "2".into());
        assert_eq!(
            eval_value(&r, "", &vars).unwrap(),
            "https://x.com/search?q=蛊真人&pg=2"
        );
    }

    #[test]
    fn firstof_falls_back_to_second_when_first_empty() {
        let r = rule(
            r#"{"firstOf":[{"via":"css","select":".nope","extract":"text"},{"via":"css","select":"h2","extract":"text"}]}"#,
        );
        let html = r#"<h2>标题</h2>"#;
        assert_eq!(eval_value(&r, html, &Vars::new()).unwrap(), "标题");
    }

    #[test]
    fn clean_regex_replace_strips_boilerplate() {
        let r = rule(
            r#"{"via":"raw","clean":[{"regex":"请收藏本站[^\\n]*","replace":""},{"trim":true}]}"#,
        );
        let out = eval_value(&r, "正文内容 请收藏本站xxx.com", &Vars::new()).unwrap();
        assert_eq!(out, "正文内容");
    }

    #[test]
    fn clean_pipeline_decrypts_content() {
        // 端到端:clean 链 cipher 步对「base64 密文」AES-CBC 解密出明文。
        // 先用 transform 造出该书源会返回的 base64 密文(模拟服务端加密)。
        use crate::source::{ByteEnc, CipherAlgo, CipherMode, CipherOp, CipherStep, Padding};
        let plain = "蛊真人 第一章 正文……";
        let ct = transform::cipher(
            plain,
            &CipherStep {
                algo: CipherAlgo::Aes,
                mode: CipherMode::Cbc,
                padding: Padding::Pkcs7,
                op: CipherOp::Encrypt,
                key: "0123456789abcdef".into(),
                key_enc: ByteEnc::Utf8,
                iv: Some("abcdef9876543210".into()),
                iv_enc: ByteEnc::Utf8,
                input_enc: Some(ByteEnc::Utf8),
                output_enc: Some(ByteEnc::Base64),
            },
        )
        .unwrap();

        // 书源侧:via:raw 取到密文,clean 用 cipher 步解密(默认 decrypt + inputEnc=base64)。
        let r = rule(
            r#"{"via":"raw","clean":[{"cipher":{"algo":"aes","mode":"cbc","key":"0123456789abcdef","iv":"abcdef9876543210"}}]}"#,
        );
        let out = eval_value(&r, &ct, &Vars::new()).unwrap();
        assert_eq!(out, plain);
    }

    #[test]
    fn clean_cipher_error_propagates() {
        // 非法 base64 密文 → cipher 步报错(显式失败,不静默空)。
        let r = rule(
            r#"{"via":"raw","clean":[{"cipher":{"algo":"aes","mode":"cbc","key":"0123456789abcdef","iv":"abcdef9876543210"}}]}"#,
        );
        let err = eval_value(&r, "!!!not-base64!!!", &Vars::new());
        assert!(matches!(
            err,
            Err(EvalError::Codec(_) | EvalError::Crypto(_))
        ));
    }

    #[test]
    fn js_rule_parses_regardless_of_feature() {
        // Rule::Js 与 clean.js 变体恒可解析(配置可移植,不受 feature 影响)。
        assert!(matches!(rule(r#"{"js":"result + '!'"}"#), Rule::Js { .. }));
        let r = rule(r#"{"via":"raw","clean":[{"js":"result"}]}"#);
        assert!(matches!(r, Rule::Leaf(_)));
    }

    #[cfg(not(feature = "js"))]
    #[test]
    fn js_rule_unsupported_without_feature() {
        let r = rule(r#"{"js":"result + '!'"}"#);
        assert!(matches!(
            eval_value(&r, "x", &Vars::new()),
            Err(EvalError::Unsupported("js"))
        ));
    }

    #[cfg(feature = "js")]
    #[test]
    fn js_rule_evaluates_with_feature() {
        let r = rule(r#"{"js":"result + '!'"}"#);
        assert_eq!(eval_value(&r, "x", &Vars::new()).unwrap(), "x!");
        // clean.js 也生效:取值后 JS 后处理。
        let r2 = rule(r#"{"via":"raw","clean":[{"js":"result.toUpperCase()"}]}"#);
        assert_eq!(eval_value(&r2, "abc", &Vars::new()).unwrap(), "ABC");
    }
}
