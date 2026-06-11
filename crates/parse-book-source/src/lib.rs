//! `parse-book-source`:AI 原生的结构化书源引擎。
//!
//! 书源是一份显式结构化 JSON(无紧凑字符串 DSL),由 [`Engine`] 驱动:
//! 搜索 / 浏览 / 书详情 / 目录(含分卷)/ 正文,并内置样例校验回路。
//! 取页经 [`Fetcher`] 端口抽象,默认 [`ReqwestFetcher`]。
//!
//! 分层(见 OpenSpec change `ai-friendly-book-source` 的 design):
//! - `model` — 纯领域类型。
//! - `source` — v2 配置(serde 镜像 `book-source.schema.json`),其中 `Rule` 既是配置、
//!   也是供求值器遍历的语法树;按 `rule`/`clean`/`http`/`op` 分文件。
//! - `eval` — 规则解释器(Interpreter + Composite),含抽取后端 `backend`(css/json/regex/raw)、
//!   `xpath` 后端、确定性算子 `transform`、JS 逃生舱 `js`。
//! - `fetch` — 取页端口(Ports & Adapters),含 `cookie` 库与浏览器反爬 `browser`。
//! - `host` — JS host 桥(`js-host`),含持久状态 `state`。
//! - `engine` — 用例(search/explore/book_info/toc/content)+ 有界分页。
//! - `verify` — 样例校验回路。
//! - `error` — 分层错误。

pub mod engine;
pub mod error;
pub mod eval;
pub mod fetch;
#[cfg(feature = "js-host")]
pub mod host;
pub mod model;
pub mod source;
pub mod verify;

// 公开面:运行时入口(Engine)+ 取页端口 + 配置 + 领域类型 + 校验 + 错误。
// 规则 AST(`Rule` 等)与求值/抽取细节在 `source` / `eval` 下,按需取用。
pub use engine::Engine;
pub use error::{BookSourceError, ConfigError, EvalError, FetchError, Result};
pub use fetch::{FetchRequest, FetchResponse, Fetcher, ReqwestFetcher, is_challenge};
pub use model::{BookInfo, BookListItem, Chapter, Toc, Volume};
pub use source::{BookSource, Category, FetchMode, UrlOrRule};
pub use verify::{Check, CheckStatus, DiagnoseReport, VerifyReport, diagnose, verify_sample};

// 兼容 re-export:聚合后保持历史公开模块路径稳定(主程序/examples 直接引用)。
// `cookie` 库归入 `fetch/`、`state` 归入 `host/`,这里保留旧顶层路径别名。
pub use fetch::cookie;
#[cfg(feature = "js-host")]
pub use host::state;

// 反爬:系统浏览器解挑战(`browser` feature)。
#[cfg(feature = "browser")]
pub use fetch::browser::{
    AuthDecision, BrowserCookie, BrowserFetcher, BrowserOptions, BrowserUi, Clearance,
    EscalatingFetcher, LoginCriteria, LoginOutcome, LoginSignal, detect_browser,
    shutdown_render_pool,
};

/// 测试共用工具:最小书源 + 一次性本地 HTTP 服务(host / browser 等模块单测共享,
/// 避免逐处复制 listener / 最小书源样板)。
#[cfg(test)]
pub(crate) mod testutil {
    // 各 feature 组合下使用方不同(host 测试在 js-host、browser 测试在 browser 下),
    // 允许部分 helper 在某些组合中未被使用。
    #![allow(dead_code)]

    use crate::source::BookSource;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    /// 最小书源(仅为构造取页器:base 指向本地测试服务)。
    pub(crate) fn book_source(base: &str) -> BookSource {
        serde_json::from_value(serde_json::json!({
            "schema": "trnovel-booksource/v2",
            "name": "t",
            "url": base,
            "bookInfo": {},
            "toc": {"list": {"via": "raw"}, "name": {"via": "raw"}, "url": {"via": "raw"}},
            "content": {"value": {"via": "raw"}}
        }))
        .expect("minimal book source")
    }

    /// 处理一次连接的回显 HTTP 服务:响应体 = 收到的原始请求(便于断言请求头)。
    pub(crate) fn spawn_echo_server() -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 8192];
                let n = stream.read(&mut buf).unwrap_or(0);
                let body = buf[..n].to_vec();
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(&body);
                let _ = stream.flush();
            }
        });
        (base, handle)
    }

    /// 处理一次连接、返回固定原始 HTTP 响应的服务(用于断言响应头/状态码透传)。
    pub(crate) fn spawn_fixed_server(
        raw_response: String,
    ) -> (String, std::thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let handle = std::thread::spawn(move || {
            if let Ok((mut stream, _)) = listener.accept() {
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(raw_response.as_bytes());
                let _ = stream.flush();
            }
        });
        (base, handle)
    }
}
