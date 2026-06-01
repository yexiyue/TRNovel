//! `doctor` 子命令:全流程验证书源 JSON,逐项打印 ✓/✗(非 TUI)。
//!
//! 用于校验 AI 生成的书源:读文件 → 构建 [`parse_book_source::Engine`] → 跑
//! [`parse_book_source::diagnose`](全流程体检)→ 打印逐项结果。所有错误都作为
//! 「配置」失败项展示,不向外冒泡。

use parse_book_source::{BookSource, Engine, diagnose};
use std::path::Path;

/// 体检指定路径的书源 JSON 并打印报告。
pub async fn run(path: &Path) {
    let json = match std::fs::read_to_string(path) {
        Ok(j) => j,
        Err(e) => return print_config_error("读取文件失败", e),
    };
    let source = match BookSource::from_json(&json) {
        Ok(s) => s,
        Err(e) => return print_config_error("JSON 解析失败", e),
    };
    let engine = match Engine::new(source) {
        Ok(e) => e,
        Err(e) => return print_config_error("构建引擎失败", e),
    };

    let report = diagnose(&engine).await;
    print!("{report}");
    if report.healthy() {
        println!("\n✓ 全部通过(○ 为未配置/跳过)");
    } else {
        println!("\n✗ 存在异常项,请检查上面标 ✗ 的规则");
    }
}

/// 在体检尚未真正开始(读文件/解析/建引擎阶段)就失败时,打成「配置」✗ 项。
fn print_config_error(what: &str, err: impl std::fmt::Display) {
    println!("书源诊断\n  ✗ 配置   {what}: {err}");
}
