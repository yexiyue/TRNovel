//! `doctor` 子命令:全流程验证书源 JSON,逐项打印 ✓/✗(非 TUI)。
//!
//! 用于校验 AI 生成的书源:读文件 → 构建 [`parse_book_source::Engine`] → 跑
//! [`parse_book_source::diagnose`] 做全流程体检 → 打印逐项结果。所有错误都作为
//! 「配置」失败项展示,不向外冒泡。

use parse_book_source::{BookSource, BrowserFetcher, BrowserOptions, Engine, diagnose};
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
    // 带浏览器构建:渲染型 op(`render-fetcher`,如番茄搜索)才能真正验证(headless 渲染 +
    // CDP 拦截);非渲染 op 不开浏览器(EscalatingFetcher 仅在 render/撞挑战时才启动)。
    // 探测不到浏览器(CI/沙箱)→ None,等同纯 reqwest:渲染 op 优雅降级标 ✗。
    let browser = BrowserFetcher::detect(BrowserOptions::default());
    let engine = match Engine::with_browser_assist(source, browser) {
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
