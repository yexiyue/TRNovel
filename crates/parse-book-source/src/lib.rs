//! `parse-book-source`:AI 原生的结构化书源引擎。
//!
//! 书源是一份显式结构化 JSON(无紧凑字符串 DSL),由 [`Engine`] 驱动:
//! 搜索 / 浏览 / 书详情 / 目录(含分卷)/ 正文,并内置样例校验回路。
//! 取页经 [`Fetcher`] 端口抽象,默认 [`ReqwestFetcher`]。
//!
//! 分层(见 OpenSpec change `ai-friendly-book-source` 的 design):
//! - `model` — 纯领域类型。
//! - `source` — v2 配置(serde 镜像 `book-source.schema.json`),其中 `Rule` 既是配置、
//!   也是供求值器遍历的语法树。
//! - `eval` — 规则解释器(Interpreter + Composite)。
//! - `backend` — 抽取后端(Strategy:css/json/regex/raw)。
//! - `fetch` — 取页端口(Ports & Adapters)。
//! - `engine` — 用例(search/explore/book_info/toc/content)+ 有界分页。
//! - `verify` — 样例校验回路。
//! - `error` — 分层错误。

pub mod backend;
#[cfg(feature = "browser")]
pub mod browser;
pub mod cookie;
pub mod engine;
pub mod error;
pub mod eval;
pub mod fetch;
#[cfg(feature = "js-host")]
pub mod host;
#[cfg(feature = "js")]
mod js;
pub mod model;
pub mod source;
#[cfg(feature = "js-host")]
pub mod state;
mod transform;
pub mod verify;
mod xpath;

// 公开面:运行时入口(Engine)+ 取页端口 + 配置 + 领域类型 + 校验 + 错误。
// 规则 AST(`Rule` 等)与求值/抽取细节在 `source` / `eval` / `backend` 下,按需取用。
pub use engine::Engine;
pub use error::{BookSourceError, ConfigError, EvalError, FetchError, Result};
pub use fetch::{FetchRequest, FetchResponse, Fetcher, ReqwestFetcher, is_challenge};
pub use model::{BookInfo, BookListItem, Chapter, Toc, Volume};
pub use source::{BookSource, Category, FetchMode, UrlOrRule};
pub use verify::{Check, CheckStatus, DiagnoseReport, VerifyReport, diagnose, verify_sample};

// 反爬:系统浏览器解挑战(`browser` feature)。
#[cfg(feature = "browser")]
pub use browser::{
    AuthDecision, BrowserFetcher, BrowserOptions, BrowserUi, Clearance, EscalatingFetcher,
    detect_browser,
};
