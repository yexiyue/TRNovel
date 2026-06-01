//! 样例校验回路:对 `samples` 跑完整流程并断言可执行不变量,返回结构化报告。
//! 同一套断言用于生成期校验与运行期监控(见 design D5)。

use super::engine::Engine;
use super::error::Result;
use super::source::Sample;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::FetchError;
    use crate::fetch::{FetchRequest, Fetcher};
    use crate::source::BookSource;
    use async_trait::async_trait;
    use std::sync::Arc;

    /// 对所有 URL 返回同一段组合 HTML(同时含 og:meta、目录、卡片、正文),
    /// 使 book_info/toc/content/explore 各取所需,离线全流程体检。
    struct MockFetcher(String);
    #[async_trait]
    impl Fetcher for MockFetcher {
        async fn fetch(&self, _req: FetchRequest) -> std::result::Result<String, FetchError> {
            Ok(self.0.clone())
        }
    }

    const HTML: &str = r#"<html><head>
        <meta property="og:novel:book_name" content="测试书">
        <meta property="og:novel:read_url" content="/toc">
      </head><body>
        <div class="module-item"><a class="module-item-title" href="/b1">书一</a></div>
        <div class="box">
          <h2 class="module-title type">第一卷</h2>
          <div class="module-row-info"><a class="module-row-text" href="/c1"><div class="module-row-title"><span>第一章</span></div></a></div>
        </div>
        <div class="article-content"><p>正文内容。</p></div>
      </body></html>"#;

    const SOURCE: &str = r#"{
      "schema":"trnovel-booksource/v2","name":"mock","url":"https://x",
      "search":{"request":{"url":{"template":"{{base}}/s?q={{key}}"}},
                "list":{"via":"css","select":".module-item"},
                "item":{"bookUrl":{"via":"css","select":".module-item-title","extract":{"attr":"href"}},"name":{"via":"css","select":".module-item-title","extract":"text"}}},
      "explore":{"categories":[{"title":"全部","url":{"template":"{{base}}/all_{{page}}"}}],
                 "list":{"via":"css","select":".module-item"},
                 "item":{"bookUrl":{"via":"css","select":".module-item-title","extract":{"attr":"href"}},"name":{"via":"css","select":".module-item-title","extract":"text"}}},
      "bookInfo":{"name":{"via":"css","select":"[property=\"og:novel:book_name\"]","extract":{"attr":"content"}},
                  "tocUrl":{"via":"css","select":"[property=\"og:novel:read_url\"]","extract":{"attr":"content"}}},
      "toc":{"list":{"via":"css","select":".box > h2.module-title.type, .box a.module-row-text"},
             "name":{"firstOf":[{"via":"css","select":".module-row-title","extract":"text"},{"via":"css","select":"h2","extract":"text"}]},
             "url":{"via":"css","select":"a","extract":{"attr":"href"}},
             "isVolume":{"via":"css","select":"h2","extract":"text"},"maxPages":1},
      "content":{"value":{"via":"css","select":".article-content","extract":"html"}},
      "samples":[{"bookUrl":"/b1","expect":{"name":"测试书"}}]
    }"#;

    #[tokio::test]
    async fn diagnose_all_capabilities_pass_offline() {
        let src = BookSource::from_json(SOURCE).unwrap();
        let engine = Engine::with_fetcher(src, Arc::new(MockFetcher(HTML.to_string())));
        let report = diagnose(&engine).await;
        assert!(report.healthy(), "应全部通过,实际: {report}");
        // 6 项检查:配置/浏览/书详情/目录/正文/搜索
        assert_eq!(report.checks.len(), 6);
        let toc = report.checks.iter().find(|c| c.name == "目录").unwrap();
        assert_eq!(toc.status, CheckStatus::Pass);
        assert!(toc.detail.contains("1 卷 / 1 章"));
    }

    #[tokio::test]
    async fn verify_sample_offline() {
        let src = BookSource::from_json(SOURCE).unwrap();
        let engine = Engine::with_fetcher(src.clone(), Arc::new(MockFetcher(HTML.to_string())));
        let report = verify_sample(&engine, &src.samples[0]).await.unwrap();
        assert!(report.passed, "failures: {:?}", report.failures);
        assert_eq!(report.name, "测试书");
        assert_eq!(report.chapters, 1);
        assert_eq!(report.volumes, 1);
    }
}

/// 一个样例的校验结果。
#[derive(Debug, Default, Clone)]
pub struct VerifyReport {
    /// 是否全部不变量通过。
    pub passed: bool,
    /// 失败项的可读描述(期望 vs 实际)。
    pub failures: Vec<String>,
    pub name: String,
    pub chapters: usize,
    pub volumes: usize,
    pub content_chars: usize,
}

/// 对单个样例跑 book_info → toc → 首章 content,并校验不变量。
pub async fn verify_sample(engine: &Engine, sample: &Sample) -> Result<VerifyReport> {
    let mut report = VerifyReport::default();

    let info = engine.book_info(&sample.book_url).await?;
    report.name = info.name.clone();
    if info.name.trim().is_empty() {
        report.failures.push("bookInfo.name 为空".into());
    }

    let toc_url = if info.toc_url.trim().is_empty() {
        sample.book_url.clone()
    } else {
        info.toc_url.clone()
    };
    let toc = engine.toc(&toc_url).await?;
    report.chapters = toc.chapters.len();
    report.volumes = toc.volumes.len();
    if toc.chapters.is_empty() {
        report.failures.push("目录无章节".into());
    }

    if let Some(first) = toc.chapters.first() {
        let content = engine.content(&first.url).await?;
        report.content_chars = content.chars().count();
    }

    let e = &sample.expect;
    if let Some(n) = &e.name
        && &info.name != n
    {
        report
            .failures
            .push(format!("name 期望 {:?},实际 {:?}", n, info.name));
    }
    if let Some(m) = e.min_chapters
        && report.chapters < m
    {
        report
            .failures
            .push(format!("章节数 {} < 期望 {}", report.chapters, m));
    }
    if let Some(v) = e.volumes
        && report.volumes != v
    {
        report
            .failures
            .push(format!("卷数 {} != 期望 {}", report.volumes, v));
    }
    if let Some(c) = e.min_content_chars
        && report.content_chars < c
    {
        report
            .failures
            .push(format!("正文 {} 字 < 期望 {}", report.content_chars, c));
    }

    report.passed = report.failures.is_empty();
    Ok(report)
}

// ───────────────────────── 全流程体检(doctor)─────────────────────────

/// 单项检查状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    /// 正常(✓)。
    Pass,
    /// 异常(✗)。
    Fail,
    /// 跳过(未配置 / 缺前置条件,○)。
    Skip,
}

impl CheckStatus {
    /// 状态符号:✓ / ✗ / ○。
    pub fn symbol(&self) -> char {
        match self {
            CheckStatus::Pass => '✓',
            CheckStatus::Fail => '✗',
            CheckStatus::Skip => '○',
        }
    }
}

/// 一项能力的检查结果。
#[derive(Debug, Clone)]
pub struct Check {
    pub name: &'static str,
    pub status: CheckStatus,
    pub detail: String,
}

impl Check {
    fn pass(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Pass,
            detail: detail.into(),
        }
    }
    fn fail(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Fail,
            detail: detail.into(),
        }
    }
    fn skip(name: &'static str, detail: impl Into<String>) -> Self {
        Self {
            name,
            status: CheckStatus::Skip,
            detail: detail.into(),
        }
    }
}

/// 体检报告:逐能力的 ✓/✗/○ 列表。
#[derive(Debug, Clone)]
pub struct DiagnoseReport {
    pub source_name: String,
    pub checks: Vec<Check>,
}

impl DiagnoseReport {
    /// 是否无任何异常项(Skip 不算失败)。
    pub fn healthy(&self) -> bool {
        self.checks.iter().all(|c| c.status != CheckStatus::Fail)
    }
}

impl std::fmt::Display for DiagnoseReport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "书源诊断:{}", self.source_name)?;
        for c in &self.checks {
            writeln!(f, "  {} {:<6} {}", c.status.symbol(), c.name, c.detail)?;
        }
        Ok(())
    }
}

/// 全流程体检:逐项跑「配置 / 浏览 / 搜索 / 书详情 / 目录 / 正文」并报告 ✓/✗/○。
///
/// 无 `samples` 时,会尝试用浏览/搜索探到的第一本书来测书详情/目录/正文,
/// 使没有样例的 AI 生成书源也能被全流程验证。
pub async fn diagnose(engine: &Engine) -> DiagnoseReport {
    engine.warmup().await;
    let src = engine.source();
    let mut checks = Vec::new();

    // 能构造出 Engine 即说明配置已成功解析。
    checks.push(Check::pass("配置", format!("书源「{}」", src.name)));

    // 浏览(同时探一个可用 book_url 供读取路径在无样例时使用)
    let mut probe_book_url: Option<String> = None;
    if src.explore.is_some() {
        match engine.explore_categories().first() {
            Some(cat) => match engine.explore(&cat.url, 1, 20).await {
                Ok(books) if !books.is_empty() => {
                    probe_book_url = books
                        .iter()
                        .find(|b| !b.book_url.is_empty())
                        .map(|b| b.book_url.clone());
                    checks.push(Check::pass(
                        "浏览",
                        format!("{} 本(分类「{}」)", books.len(), cat.title),
                    ));
                }
                Ok(_) => checks.push(Check::fail("浏览", "结果为空")),
                Err(e) => checks.push(Check::fail("浏览", e.to_string())),
            },
            None => checks.push(Check::skip("浏览", "未配置分类")),
        }
    } else {
        checks.push(Check::skip("浏览", "未配置"));
    }

    // 读取路径(书详情→目录→正文)先于搜索:这是最可靠的链路,
    // 避免被搜索可能触发的反爬(如 Cloudflare)影响后续请求。
    let book_url = src
        .samples
        .first()
        .map(|s| s.book_url.clone())
        .or(probe_book_url);
    read_path_checks(engine, book_url, &mut checks).await;

    // 搜索最后做(最易触发反爬;失败也不影响其它检查项)
    if src.search.is_some() {
        match src.samples.first().and_then(|s| s.expect.name.clone()) {
            Some(q) => match engine.search(&q, 1, 20).await {
                Ok(books) if !books.is_empty() => {
                    checks.push(Check::pass("搜索", format!("「{q}」→ {} 本", books.len())))
                }
                Ok(_) => checks.push(Check::fail("搜索", format!("「{q}」无结果"))),
                Err(e) => checks.push(Check::fail("搜索", e.to_string())),
            },
            None => checks.push(Check::skip("搜索", "无样例查询词 samples[].expect.name")),
        }
    } else {
        checks.push(Check::skip("搜索", "未配置"));
    }

    DiagnoseReport {
        source_name: src.name.clone(),
        checks,
    }
}

/// 读取路径检查:书详情 → 目录 → 正文。把结果追加到 `checks`(失败不 panic、不冒泡)。
async fn read_path_checks(engine: &Engine, book_url: Option<String>, checks: &mut Vec<Check>) {
    let Some(book_url) = book_url else {
        checks.push(Check::skip("书详情", "无 book_url(需 samples 或可浏览)"));
        checks.push(Check::skip("目录", "无 book_url"));
        checks.push(Check::skip("正文", "无 book_url"));
        return;
    };

    let info = match engine.book_info(&book_url).await {
        Ok(info) if !info.name.trim().is_empty() => {
            checks.push(Check::pass("书详情", info.name.clone()));
            info
        }
        Ok(_) => {
            checks.push(Check::fail("书详情", "name 为空"));
            checks.push(Check::skip("目录", "书详情失败"));
            checks.push(Check::skip("正文", "书详情失败"));
            return;
        }
        Err(e) => {
            checks.push(Check::fail("书详情", e.to_string()));
            checks.push(Check::skip("目录", "书详情失败"));
            checks.push(Check::skip("正文", "书详情失败"));
            return;
        }
    };

    let toc_url = if info.toc_url.trim().is_empty() {
        book_url
    } else {
        info.toc_url
    };
    let first_chapter_url = match engine.toc(&toc_url).await {
        Ok(toc) if !toc.chapters.is_empty() => {
            checks.push(Check::pass(
                "目录",
                format!("{} 卷 / {} 章", toc.volumes.len(), toc.chapters.len()),
            ));
            Some(toc.chapters[0].url.clone())
        }
        Ok(_) => {
            checks.push(Check::fail("目录", "无章节"));
            None
        }
        Err(e) => {
            checks.push(Check::fail("目录", e.to_string()));
            None
        }
    };

    match first_chapter_url {
        Some(url) => match engine.content(&url).await {
            Ok(c) if c.trim().chars().count() >= 1 => {
                checks.push(Check::pass("正文", format!("{} 字", c.chars().count())))
            }
            Ok(_) => checks.push(Check::fail("正文", "正文为空")),
            Err(e) => checks.push(Check::fail("正文", e.to_string())),
        },
        None => checks.push(Check::skip("正文", "目录无可用章节")),
    }
}
