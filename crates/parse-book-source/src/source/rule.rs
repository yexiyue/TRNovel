//! 规则 AST:抽取语义(`Via`/`Extract`)、叶子规则与组合子(`Rule`)。
//!
//! `Rule` 既是配置、也是供求值器([`crate::eval`])遍历的语法树(见 design D1/D6)。

use super::clean::CleanStep;
use serde::{Deserialize, Serialize};

/// 抽取后端(决定 `select` 的语义)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Via {
    #[default]
    Css,
    Xpath,
    Json,
    Regex,
    /// 直接使用当前上下文值(只跑 clean)。
    Raw,
}

/// 取值方式(枚举字符串 或 `{ "attr": "..." }`)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(untagged)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Extract {
    Op(ExtractOp),
    Attr { attr: String },
}

impl Default for Extract {
    fn default() -> Self {
        Extract::Op(ExtractOp::Text)
    }
}

/// 文本/HTML 取值算子。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ExtractOp {
    #[default]
    Text,
    OwnText,
    Html,
    InnerHtml,
    OuterHtml,
}

/// 叶子规则:在当前上下文做一次抽取。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct LeafRule {
    #[serde(default)]
    pub via: Via,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<i64>,
    #[serde(default)]
    pub extract: Extract,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub clean: Vec<CleanStep>,
}

/// 一条规则:叶子,或组合子。组合子按其唯一键判别(见 design D1)。
///
/// 反序列化时按变体顺序尝试:组合子(各有唯一必填键)在前,叶子兜底。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(untagged)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Rule {
    /// 取首个非空子规则结果(回退/自愈)。
    FirstOf {
        #[serde(rename = "firstOf")]
        first_of: Vec<Rule>,
    },
    /// 拼接非空子规则结果。
    Concat {
        concat: Vec<Rule>,
        #[serde(default)]
        join: String,
    },
    /// 字面量。
    Literal { literal: String },
    /// 模板插值(`{{key}}`/`{{page}}`/命名变量)。
    Template { template: String },
    /// JS 逻辑编排逃生舱(值规则):以当前上下文为 `result`、注入 `baseUrl`/变量 + `crypto`
    /// 助手求值,返回字符串。求值需启用 `js` feature(否则返回 `Unsupported("js")`)。
    /// 必须置于 `Leaf` 之前——`js` 是其唯一判别键,否则会被全可选的 `Leaf` 吞掉。
    Js { js: String },
    /// 叶子(兜底)。
    Leaf(LeafRule),
}

/// URL 字段:可为字符串模板,或一条规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(untagged)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum UrlOrRule {
    Str(String),
    Rule(Box<Rule>),
}
