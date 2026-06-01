//! 用例引擎(Template Method + Paginator)。五个操作共享「取页 → 选列表/值 → 映射 →
//! 可选有界分页」骨架;`Engine` 廉价 `Clone`(内部 `Arc`),操作不跨 await 持锁(D10)。

use super::error::{BookSourceError, Result};
use super::eval::{Vars, eval_list, eval_value};
use super::fetch::{FetchRequest, Fetcher, ReqwestFetcher};
use super::model::{BookInfo, BookListItem, Chapter, Toc, Volume};
use super::source::{BookRules, BookSource, Category, Rule, UrlOrRule};
use std::sync::Arc;

/// 书源运行时引擎。
#[derive(Clone)]
pub struct Engine {
    source: Arc<BookSource>,
    fetcher: Arc<dyn Fetcher>,
}

impl std::fmt::Debug for Engine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Engine")
            .field("source", &self.source.name)
            .finish_non_exhaustive()
    }
}

impl Engine {
    /// 用默认 reqwest 取页后端构建。
    pub fn new(source: BookSource) -> Result<Self> {
        let fetcher = Arc::new(ReqwestFetcher::new(&source)?);
        Ok(Self {
            source: Arc::new(source),
            fetcher,
        })
    }

    /// 注入自定义取页后端(便于测试替身 / 反爬适配器)。
    pub fn with_fetcher(source: BookSource, fetcher: Arc<dyn Fetcher>) -> Self {
        Self {
            source: Arc::new(source),
            fetcher,
        }
    }

    /// 暴露只读配置。
    pub fn source(&self) -> &BookSource {
        &self.source
    }

    fn base_vars(&self) -> Vars {
        let mut v = Vars::new();
        v.insert(
            "base".into(),
            self.source.url.trim_end_matches('/').to_string(),
        );
        v
    }

    /// 预热:按 `http.warmup` 先访问若干页以累积会话 cookie(失败忽略)。
    pub async fn warmup(&self) {
        for u in &self.source.http.warmup {
            let _ = self.fetcher.fetch(FetchRequest::get(u.clone())).await;
        }
    }

    /// 书籍详情。
    pub async fn book_info(&self, book_url: &str) -> Result<BookInfo> {
        let html = self.fetcher.fetch(FetchRequest::get(book_url)).await?;
        let vars = self.base_vars();
        self.eval_book_info(&self.source.book_info, &html, &vars)
    }

    /// 目录(章节 + 分卷),支持有界分页。
    pub async fn toc(&self, toc_url: &str) -> Result<Toc> {
        let toc = &self.source.toc;
        let vars = self.base_vars();
        let pages = self
            .fetch_pages(toc_url, toc.next_page.as_ref(), toc.max_pages, &vars)
            .await?;

        let mut chapters: Vec<Chapter> = Vec::new();
        let mut volumes: Vec<Volume> = Vec::new();
        for page in &pages {
            for item in eval_list(&toc.list, page)? {
                let title = eval_value(&toc.name, &item, &vars)?;
                let is_volume = match &toc.is_volume {
                    Some(r) => !eval_value(r, &item, &vars)?.trim().is_empty(),
                    None => false,
                };
                if is_volume {
                    volumes.push(Volume {
                        title,
                        first_chapter_index: chapters.len(),
                    });
                } else {
                    let url = eval_value(&toc.url, &item, &vars)?;
                    chapters.push(Chapter {
                        title,
                        url,
                        is_volume: false,
                    });
                }
            }
        }
        Ok(Toc { chapters, volumes })
    }

    /// 正文,支持有界分页。
    pub async fn content(&self, chapter_url: &str) -> Result<String> {
        let c = &self.source.content;
        let vars = self.base_vars();
        let pages = self
            .fetch_pages(chapter_url, c.next_page.as_ref(), c.max_pages, &vars)
            .await?;
        let mut parts = Vec::with_capacity(pages.len());
        for page in &pages {
            parts.push(eval_value(&c.value, page, &vars)?);
        }
        Ok(parts.join("\n"))
    }

    /// 搜索。
    pub async fn search(&self, key: &str, page: u32, page_size: u32) -> Result<Vec<BookListItem>> {
        let op = self
            .source
            .search
            .as_ref()
            .ok_or(BookSourceError::Missing("search"))?;
        let mut vars = self.base_vars();
        vars.insert("key".into(), key.to_string());
        vars.insert("page".into(), page.to_string());
        vars.insert("pageSize".into(), page_size.to_string());

        let url = self.resolve_url(&op.request.url, &vars)?;
        let body = match &op.request.body {
            Some(b) => Some(self.resolve_url(b, &vars)?),
            None => None,
        };
        let html = self
            .fetcher
            .fetch(FetchRequest {
                url,
                method: op.request.method,
                body,
                headers: op.request.headers.clone(),
            })
            .await?;
        self.eval_list_items(&op.list, &op.item, &html)
    }

    /// 浏览某分类的某一页。
    pub async fn explore(
        &self,
        category_url: &UrlOrRule,
        page: u32,
        page_size: u32,
    ) -> Result<Vec<BookListItem>> {
        let op = self
            .source
            .explore
            .as_ref()
            .ok_or(BookSourceError::Missing("explore"))?;
        let mut vars = self.base_vars();
        vars.insert("page".into(), page.to_string());
        vars.insert("pageSize".into(), page_size.to_string());
        let url = self.resolve_url(category_url, &vars)?;
        let html = self.fetcher.fetch(FetchRequest::get(url)).await?;
        self.eval_list_items(&op.list, &op.item, &html)
    }

    /// 浏览分类列表,供上层选择后翻页。
    pub fn explore_categories(&self) -> Vec<Category> {
        self.source
            .explore
            .as_ref()
            .map(|e| e.categories.clone())
            .unwrap_or_default()
    }

    // ── 内部 ──

    /// 有界分页抓取:从 `start` 起,若 `next_page` 求值得非空 URL 则续抓,直到为空或达 `max_pages`。
    async fn fetch_pages(
        &self,
        start: &str,
        next_page: Option<&Rule>,
        max_pages: u32,
        vars: &Vars,
    ) -> Result<Vec<String>> {
        let mut pages = Vec::new();
        let mut url = start.to_string();
        for _ in 0..max_pages.max(1) {
            let html = self.fetcher.fetch(FetchRequest::get(url.clone())).await?;
            let next = match next_page {
                Some(r) => eval_value(r, &html, vars)?,
                None => String::new(),
            };
            pages.push(html);
            if next.trim().is_empty() {
                break;
            }
            url = next;
        }
        Ok(pages)
    }

    fn eval_list_items(
        &self,
        list: &Rule,
        item: &BookRules,
        html: &str,
    ) -> Result<Vec<BookListItem>> {
        let vars = self.base_vars();
        let mut out = Vec::new();
        for ctx in eval_list(list, html)? {
            let info = self.eval_book_info(item, &ctx, &vars)?;
            let book_url = opt_eval(item.book_url.as_ref(), &ctx, &vars)?;
            out.push(BookListItem { info, book_url });
        }
        Ok(out)
    }

    fn eval_book_info(&self, r: &BookRules, ctx: &str, vars: &Vars) -> Result<BookInfo> {
        Ok(BookInfo {
            name: opt_eval(r.name.as_ref(), ctx, vars)?,
            author: opt_eval(r.author.as_ref(), ctx, vars)?,
            cover: opt_eval(r.cover.as_ref(), ctx, vars)?,
            intro: opt_eval(r.intro.as_ref(), ctx, vars)?,
            kind: opt_eval(r.kind.as_ref(), ctx, vars)?,
            last_chapter: opt_eval(r.last_chapter.as_ref(), ctx, vars)?,
            toc_url: opt_eval(r.toc_url.as_ref(), ctx, vars)?,
            word_count: opt_eval(r.word_count.as_ref(), ctx, vars)?,
        })
    }

    fn resolve_url(&self, u: &UrlOrRule, vars: &Vars) -> Result<String> {
        Ok(match u {
            // 字符串按模板插值({{base}}/{{key}}/{{page}} 等)。
            UrlOrRule::Str(s) => eval_value(
                &Rule::Template {
                    template: s.clone(),
                },
                "",
                vars,
            )?,
            UrlOrRule::Rule(r) => eval_value(r, "", vars)?,
        })
    }
}

/// 求值一个可选规则;None 或空 → 空串。
fn opt_eval(rule: Option<&Rule>, ctx: &str, vars: &Vars) -> Result<String> {
    Ok(match rule {
        Some(r) => eval_value(r, ctx, vars)?,
        None => String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::FetchError;
    use crate::fetch::Fetcher;
    use async_trait::async_trait;

    /// 注入固定 HTML 的取页替身,使引擎可离线单测(D9)。
    struct MockFetcher(String);

    #[async_trait]
    impl Fetcher for MockFetcher {
        async fn fetch(&self, _req: FetchRequest) -> std::result::Result<String, FetchError> {
            Ok(self.0.clone())
        }
    }

    const CATALOG: &str = r#"<html><body><div class="box">
        <span id="shuqian"><h2 class="module-title type">阅读进度</h2></span>
        <h2 class="module-title type">第一卷</h2>
        <div class="module-row-info"><a class="module-row-text" href="/n/1.html"><div class="module-row-title"><span>第一章</span></div></a></div>
        <div class="module-row-info"><a class="module-row-text" href="/n/2.html"><div class="module-row-title"><span>第二章</span></div></a></div>
        <h2 class="module-title type">第二卷</h2>
        <div class="module-row-info"><a class="module-row-text" href="/n/3.html"><div class="module-row-title"><span>第三章</span></div></a></div>
        </div></body></html>"#;

    const SOURCE: &str = r#"{
      "schema":"trnovel-booksource/v2","name":"t","url":"https://x",
      "bookInfo":{},
      "toc":{
        "list":{"via":"css","select":".box > h2.module-title.type, .box a.module-row-text"},
        "name":{"firstOf":[{"via":"css","select":".module-row-title","extract":"text"},{"via":"css","select":"h2","extract":"text"}]},
        "url":{"via":"css","select":"a","extract":{"attr":"href"}},
        "isVolume":{"via":"css","select":"h2","extract":"text"},
        "maxPages":1
      },
      "content":{"value":{"via":"css","select":".article-content","extract":"text"}}
    }"#;

    #[tokio::test]
    async fn engine_toc_splits_volumes_offline() {
        let src = BookSource::from_json(SOURCE).unwrap();
        let engine = Engine::with_fetcher(src, Arc::new(MockFetcher(CATALOG.to_string())));
        let toc = engine.toc("/any").await.unwrap();
        assert_eq!(toc.volumes.len(), 2, "应识别 2 卷");
        assert_eq!(toc.chapters.len(), 3, "应识别 3 章");
        assert_eq!(toc.chapters[0].title, "第一章");
        assert_eq!(toc.chapters[0].url, "/n/1.html");
        assert_eq!(toc.volumes[1].first_chapter_index, 2);
    }
}
