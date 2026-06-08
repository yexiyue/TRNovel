//! 用例引擎(Template Method + Paginator)。五个操作共享「取页 → 选列表/值 → 映射 →
//! 可选有界分页」骨架;`Engine` 廉价 `Clone`(内部 `Arc`),操作不跨 await 持锁(D10)。

use super::error::{BookSourceError, Result};
use super::eval::{Vars, eval_list, eval_value};
use super::fetch::{FetchRequest, Fetcher, ReqwestFetcher};
use super::model::{BookInfo, BookListItem, Chapter, Toc, Volume};
use super::source::{BookRules, BookSource, Category, Rule, UrlOrRule};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// 书源运行时引擎。
#[derive(Clone)]
pub struct Engine {
    source: Arc<BookSource>,
    fetcher: Arc<dyn Fetcher>,
    /// 登录态请求头(JWT/自定义头/Cookie 同路径),并入引擎构造的每个请求。
    /// 由调用方在登录后经 [`Engine::with_login_header`] 注入(来自 per-source 状态)。
    login_header: BTreeMap<String, String>,
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
            login_header: BTreeMap::new(),
        })
    }

    /// 注入自定义取页后端(便于测试替身 / 反爬适配器)。
    pub fn with_fetcher(source: BookSource, fetcher: Arc<dyn Fetcher>) -> Self {
        Self {
            source: Arc::new(source),
            fetcher,
            login_header: BTreeMap::new(),
        }
    }

    /// 注入登录态请求头(登录后由调用方从 per-source 状态取出)。链式构造:
    /// `Engine::new(src)?.with_login_header(state.login_header)`。空 map 等同未登录。
    #[must_use]
    pub fn with_login_header(mut self, login_header: BTreeMap<String, String>) -> Self {
        self.login_header = login_header;
        self
    }

    /// 用「升级式取页」构建(`browser` feature):平时 reqwest,撞挑战且 `browser` 为
    /// `Some` 时升级解挑战。是否传入浏览器(书源 `http.fetcher` ∧ 用户授权 ∧ 探测到)
    /// 的策略由调用方(app)决定;`None` 等同纯 reqwest(撞挑战即降级)。
    #[cfg(feature = "browser")]
    pub fn with_browser_assist(
        source: BookSource,
        browser: Option<crate::browser::BrowserFetcher>,
    ) -> Result<Self> {
        let fetcher = crate::browser::EscalatingFetcher::new(&source, browser)?;
        Ok(Self {
            source: Arc::new(source),
            fetcher: Arc::new(fetcher),
            login_header: BTreeMap::new(),
        })
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

    /// 构造一个带 loginHeader 的 GET 请求——引擎所有取页统一经此并入登录态。
    fn get_req(&self, url: impl Into<String>) -> FetchRequest {
        let mut req = FetchRequest::get(url);
        self.merge_login_header(&mut req.headers);
        req
    }

    /// 把 loginHeader 并入请求头(合并的最后一层:登录态覆盖书源同名头)。
    fn merge_login_header(&self, headers: &mut HashMap<String, String>) {
        for (k, v) in &self.login_header {
            headers.insert(k.clone(), v.clone());
        }
    }

    /// 取页(带 loginHeader)后跑 `loginCheckJs` 校验登录态:失效则返回 [`BookSourceError::LoginExpired`]。
    async fn fetch_checked(&self, url: impl Into<String>) -> Result<String> {
        let html = self.fetcher.fetch(self.get_req(url)).await?;
        self.check_login(&html)?;
        Ok(html)
    }

    /// `loginCheckJs`(响应期登录态校验,D10 第一版):脚本以 `result`=响应求值;
    /// 返回空 / `false` / `0` 视为登录失效 → 抛 [`BookSourceError::LoginExpired`] 提示用户重登。
    /// 空脚本或未启用 `js` feature 时为 no-op。
    fn check_login(&self, response: &str) -> Result<()> {
        let js = self.source.login_check_js.trim();
        if js.is_empty() {
            return Ok(());
        }
        #[cfg(feature = "js")]
        {
            let vars = self.base_vars();
            let verdict = eval_value(&Rule::Js { js: js.to_string() }, response, &vars)?;
            if matches!(verdict.trim(), "" | "false" | "0") {
                return Err(BookSourceError::LoginExpired);
            }
        }
        let _ = response;
        Ok(())
    }

    /// 预热:按 `http.warmup` 先访问若干页以累积会话 cookie(失败忽略)。
    pub async fn warmup(&self) {
        for u in &self.source.http.warmup {
            let _ = self.fetcher.fetch(self.get_req(u.clone())).await;
        }
    }

    /// 书籍详情。
    pub async fn book_info(&self, book_url: &str) -> Result<BookInfo> {
        let html = self.fetch_checked(book_url).await?;
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
        let mut headers = op.request.headers.clone();
        self.merge_login_header(&mut headers);
        let html = self
            .fetcher
            .fetch(FetchRequest {
                url,
                method: op.request.method,
                body,
                headers,
            })
            .await?;
        self.check_login(&html)?;
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
        let html = self.fetch_checked(url).await?;
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
            let html = self.fetch_checked(url.clone()).await?;
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

    use std::sync::Mutex;

    /// 注入固定 HTML 的取页替身,使引擎可离线单测(D9)。
    struct MockFetcher(String);

    #[async_trait]
    impl Fetcher for MockFetcher {
        async fn fetch(&self, _req: FetchRequest) -> std::result::Result<String, FetchError> {
            Ok(self.0.clone())
        }
    }

    /// 记录最近一次请求头的取页替身(验证引擎是否并入 loginHeader)。
    struct RecordingFetcher {
        body: String,
        last_headers: Arc<Mutex<HashMap<String, String>>>,
    }

    #[async_trait]
    impl Fetcher for RecordingFetcher {
        async fn fetch(&self, req: FetchRequest) -> std::result::Result<String, FetchError> {
            *self.last_headers.lock().unwrap() = req.headers;
            Ok(self.body.clone())
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

    // ── 6.3/6.4:引擎构造的请求并入 loginHeader(JWT/Cookie 同路径)──
    #[tokio::test]
    async fn engine_merges_login_header_into_requests() {
        let src = BookSource::from_json(SOURCE).unwrap();
        let captured = Arc::new(Mutex::new(HashMap::new()));
        let fetcher = Arc::new(RecordingFetcher {
            body: CATALOG.to_string(),
            last_headers: captured.clone(),
        });
        let mut lh = BTreeMap::new();
        lh.insert("Authorization".into(), "Bearer T".into());
        lh.insert("Cookie".into(), "sid=1".into());
        let engine = Engine::with_fetcher(src, fetcher).with_login_header(lh);

        // 任一取页路径都应带上 loginHeader(此处走 toc → fetch_pages → get_req)。
        engine.toc("/any").await.unwrap();
        let h = captured.lock().unwrap();
        assert_eq!(
            h.get("Authorization").map(String::as_str),
            Some("Bearer T"),
            "JWT 应每请求携带"
        );
        assert_eq!(
            h.get("Cookie").map(String::as_str),
            Some("sid=1"),
            "Cookie 走同一注入路径"
        );
    }

    // 未登录(空 loginHeader)时不注入任何额外头(向后兼容)。
    #[tokio::test]
    async fn engine_without_login_header_adds_nothing() {
        let src = BookSource::from_json(SOURCE).unwrap();
        let captured = Arc::new(Mutex::new(HashMap::new()));
        let fetcher = Arc::new(RecordingFetcher {
            body: CATALOG.to_string(),
            last_headers: captured.clone(),
        });
        let engine = Engine::with_fetcher(src, fetcher);
        engine.toc("/any").await.unwrap();
        assert!(captured.lock().unwrap().is_empty(), "未登录不应注入额外头");
    }

    // ── 12.2:loginCheckJs 在响应期判定登录失效 → LoginExpired ──
    #[cfg(feature = "js")]
    #[tokio::test]
    async fn login_check_js_detects_expired() {
        let json = SOURCE.replacen(
            "\"bookInfo\":{}",
            "\"loginCheckJs\":\"result.indexOf('未登录')<0\",\"bookInfo\":{}",
            1,
        );
        let src = BookSource::from_json(&json).unwrap();
        // 响应含「未登录」→ 判失效。
        let bad = Engine::with_fetcher(
            src.clone(),
            Arc::new(MockFetcher("<html>未登录</html>".into())),
        );
        let err = bad.toc("/any").await.unwrap_err();
        assert!(err.is_login_expired(), "应判登录失效: {err}");
        // 正常响应(无「未登录」)→ 放行。
        let ok = Engine::with_fetcher(src, Arc::new(MockFetcher(CATALOG.to_string())));
        assert!(ok.toc("/any").await.is_ok(), "正常响应不应判失效");
    }
}
