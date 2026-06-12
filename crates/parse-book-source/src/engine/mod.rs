//! 用例引擎(Template Method + Paginator)。五个操作共享「取页 → 选列表/值 → 映射 →
//! 可选有界分页」骨架;`Engine` 廉价 `Clone`(内部 `Arc`),操作不跨 await 持锁(D10)。
//!
//! 本文件是公开面(构造器 / 访问器 / 五个操作 / 预热);共享的私有管线在 `internal` 子模块。

mod internal;
#[cfg(test)]
mod tests;

use crate::error::{BookSourceError, Result};
use crate::eval::{eval_list, eval_value};
use crate::fetch::cookie::CookieJar;
use crate::fetch::{Fetcher, ReqwestFetcher};
use crate::model::{BookInfo, BookList, Chapter, Toc, Volume};
use crate::source::{BookSource, Category, Method, UrlOrRule};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

/// [`Engine::send_templated_full`] 的渲染 + 点击翻页参数束(避免十参数):`render-fetcher` 的
/// `render`/`ready_for`/`intercept_api` + `search-click-pagination` 的目标页 `page` 与「下一页」
/// 选择器 `page_by`。普通请求(如 prelude)用 `RenderArgs::default()`(全关闭 = 现状 reqwest 单页)。
#[derive(Clone, Copy, Default)]
struct RenderArgs<'a> {
    render: bool,
    ready_for: Option<&'a str>,
    intercept_api: Option<&'a str>,
    /// 目标页码(1 基);仅 render+intercept 且 `> 1` + `page_by` 有值时驱动点击翻页。
    page: u32,
    /// 「下一页」CSS 选择器(`pageBy.click`)。
    page_by: Option<&'a str>,
}

/// 书源运行时引擎。
#[derive(Clone)]
pub struct Engine {
    source: Arc<BookSource>,
    fetcher: Arc<dyn Fetcher>,
    /// 登录态请求头(JWT/自定义头/Cookie 同路径),并入引擎构造的每个**同注册域**请求
    /// (跨注册域请求跳过,防页面内容诱导的第三方 URL 外泄凭据,见 [`crate::fetch::cookie::merge_login_into_headers`])。
    /// 由调用方在登录后经 [`Engine::with_login_header`] 注入(来自 per-source 状态)。
    login_header: BTreeMap<String, String>,
    /// cookie 库(按注册域,session/persistent 分离):请求前合并进 `Cookie` 头,
    /// `enabledCookieJar` 时响应 `Set-Cookie` 回灌。`Arc<RwLock>` 使 `Clone` 的引擎共享同一库。
    cookies: Arc<RwLock<CookieJar>>,
    /// 书源级捕获变量(`scope=source`,D7-bis):跨 op 共享(随 `Clone` 的引擎共享),
    /// flatten 时最低优先级;适合站级常量。
    source_vars: Arc<RwLock<BTreeMap<String, String>>>,
    /// 书籍级捕获变量(`scope=book`,D7-bis):per-book,由 app 经 [`Engine::with_book_vars`]
    /// 注入、[`Engine::book_vars`] 导出(随 per-book 快照持久化)。
    book_vars: Arc<RwLock<BTreeMap<String, String>>>,
    /// **渲染取页**的搜索/浏览结果缓存(`键 -> BookList`,`search-click-pagination` 后续):使 UI
    /// 回翻 / 重访已取页**无需重新驱动浏览器**(render 点击翻页 O(N) 点击成本只付一次)。键含
    /// 操作 + 词/分类 + 页 + 页大小。随 `Clone` 的引擎共享(`Arc`),**per-source 会话级**(引擎重建即
    /// 清空)。**仅 render 路径缓存**:reqwest 取页便宜,且缓存它会跳过 cookie 回灌 / 命名捕获等可观察
    /// 副作用(故不缓存,保持现状语义)。
    page_cache: Arc<RwLock<HashMap<String, BookList>>>,
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
        Ok(Self::with_fetcher(source, fetcher))
    }

    /// 注入自定义取页后端(测试替身 / 反爬适配器)构建。
    /// 这是**唯一真实构造器**:共享字段(登录态/cookie 库/作用域变量)的默认初始化只在此处,
    /// 其余构造器([`Engine::new`] / `with_browser_assist`)一律委托,避免新增字段漏改。
    pub fn with_fetcher(source: BookSource, fetcher: Arc<dyn Fetcher>) -> Self {
        Self {
            source: Arc::new(source),
            fetcher,
            login_header: BTreeMap::new(),
            cookies: Arc::new(RwLock::new(CookieJar::default())),
            source_vars: Arc::new(RwLock::new(BTreeMap::new())),
            book_vars: Arc::new(RwLock::new(BTreeMap::new())),
            page_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 读渲染结果缓存(命中返回克隆)。键由 [`Engine::search`]/[`Engine::explore`] 构造(含页码)。
    fn cached_page(&self, key: &str) -> Option<BookList> {
        self.page_cache
            .read()
            .ok()
            .and_then(|c| c.get(key).cloned())
    }

    /// 写渲染结果缓存(取页成功后)。锁竞争失败则静默跳过(缓存仅为优化,丢一条不影响正确性)。
    fn cache_page(&self, key: String, list: &BookList) {
        if let Ok(mut c) = self.page_cache.write() {
            c.insert(key, list.clone());
        }
    }

    /// 注入登录态请求头(登录后由调用方从 per-source 状态取出)。链式构造:
    /// `Engine::new(src)?.with_login_header(state.login_header)`。空 map 等同未登录。
    #[must_use]
    pub fn with_login_header(mut self, login_header: BTreeMap<String, String>) -> Self {
        self.login_header = login_header;
        self
    }

    /// 用持久化 cookie(`注册域 -> "k=v"`,来自 per-source 状态)初始化 cookie 库。链式构造。
    #[must_use]
    pub fn with_cookies(self, persistent: &BTreeMap<String, String>) -> Self {
        if let Ok(mut jar) = self.cookies.write() {
            *jar = CookieJar::from_persistent(persistent);
        }
        self
    }

    /// 导出当前 cookie 库中的 **persistent** cookie(`注册域 -> "k=v"`),供调用方落盘。
    /// session cookie 不导出(重启失效)。
    pub fn persistent_cookies(&self) -> BTreeMap<String, String> {
        self.cookies
            .read()
            .map(|j| j.persistent())
            .unwrap_or_default()
    }

    /// 注入书籍级捕获变量(`scope=book`,来自 per-book 快照)。链式构造(贴 [`Engine::with_login_header`])。
    #[must_use]
    pub fn with_book_vars(self, book_vars: BTreeMap<String, String>) -> Self {
        if let Ok(mut g) = self.book_vars.write() {
            *g = book_vars;
        }
        self
    }

    /// 合并书源级捕获变量(`scope=source`,来自 per-source 状态)。链式构造。
    #[must_use]
    pub fn with_source_vars(self, source_vars: &BTreeMap<String, String>) -> Self {
        if let Ok(mut g) = self.source_vars.write() {
            for (k, v) in source_vars {
                g.insert(k.clone(), v.clone());
            }
        }
        self
    }

    /// 导出书籍级捕获变量,供 app 随 per-book 快照落盘(`scope=book` 跨会话复用的承载)。
    pub fn book_vars(&self) -> BTreeMap<String, String> {
        self.book_vars.read().map(|g| g.clone()).unwrap_or_default()
    }

    /// 导出书源级捕获变量,供 app 落盘(可选;默认构建为进程内)。
    pub fn source_vars(&self) -> BTreeMap<String, String> {
        self.source_vars
            .read()
            .map(|g| g.clone())
            .unwrap_or_default()
    }

    /// 用「升级式取页」构建(`browser` feature):平时 reqwest,撞挑战且 `browser` 为
    /// `Some` 时升级解挑战。是否传入浏览器(书源 `http.fetcher` ∧ 用户授权 ∧ 探测到)
    /// 的策略由调用方(app)决定;`None` 等同纯 reqwest(撞挑战即降级)。
    #[cfg(feature = "browser")]
    pub fn with_browser_assist(
        source: BookSource,
        browser: Option<crate::fetch::browser::BrowserFetcher>,
    ) -> Result<Self> {
        let fetcher = crate::fetch::browser::EscalatingFetcher::new(&source, browser)?;
        Ok(Self::with_fetcher(source, Arc::new(fetcher)))
    }

    /// 暴露只读配置。
    pub fn source(&self) -> &BookSource {
        &self.source
    }

    /// 书源 URL(per-source 登录态文件的 key,供 app 回写 persistent cookie 落盘时定位)。
    pub fn source_url(&self) -> &str {
        &self.source.url
    }

    /// 预热:按 `http.warmup` 先访问若干页以累积会话 cookie(失败忽略)。
    /// 走 `Engine::run_request` 统一管线——`enabledCookieJar` 时预热页的 `Set-Cookie`
    /// 才会回灌引擎 cookie 库(`loginCheckJs` 在预热页可能误判,但错误被吞,不影响预热语义)。
    pub async fn warmup(&self) {
        for u in &self.source.http.warmup {
            let _ = self.run_request(self.get_req(u.clone())).await;
        }
    }

    /// 书籍详情(可选前置请求链 → 取详情页 → 抽取)。
    pub async fn book_info(&self, book_url: &str) -> Result<BookInfo> {
        let mut chapter = self.base_vars();
        self.run_prelude(&self.source.book_info.prelude, &mut chapter)
            .await?;
        let html = self.fetch_checked(book_url).await?;
        let rules = self.source.book_info.as_book_rules();
        self.eval_book_info(&rules, &html, &self.flatten(&chapter))
    }

    /// 目录(章节 + 分卷),支持前置请求链 + 有界分页。
    pub async fn toc(&self, toc_url: &str) -> Result<Toc> {
        let toc = &self.source.toc;
        let mut chapter = self.base_vars();
        self.run_prelude(&toc.prelude, &mut chapter).await?;
        let vars = self.flatten(&chapter);
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

    /// 正文,支持前置请求链 + 有界分页。
    pub async fn content(&self, chapter_url: &str) -> Result<String> {
        let c = &self.source.content;
        let mut chapter = self.base_vars();
        self.run_prelude(&c.prelude, &mut chapter).await?;
        let vars = self.flatten(&chapter);
        let pages = self
            .fetch_pages(chapter_url, c.next_page.as_ref(), c.max_pages, &vars)
            .await?;
        let mut parts = Vec::with_capacity(pages.len());
        for page in &pages {
            parts.push(eval_value(&c.value, page, &vars)?);
        }
        Ok(parts.join("\n"))
    }

    /// 搜索。返回书列表 + 可选精确总页数(`render-dual-source`)。
    pub async fn search(&self, key: &str, page: u32, page_size: u32) -> Result<BookList> {
        let op = self
            .source
            .search
            .as_ref()
            .ok_or(BookSourceError::Missing("search"))?;
        // 渲染结果缓存(仅 render 路径,见字段注释):同 (词,页,页大小) 命中即返回,免重新驱动
        // 浏览器点击翻页(UI 回翻/重访的 O(N) 点击成本只在首访付一次)。
        let cache_key = op
            .request
            .render
            .then(|| format!("s\u{0}{key}\u{0}{page}\u{0}{page_size}"));
        if let Some(hit) = cache_key.as_deref().and_then(|k| self.cached_page(k)) {
            return Ok(hit);
        }
        let mut chapter = self.base_vars();
        chapter.insert("key".into(), key.to_string());
        chapter.insert("page".into(), page.to_string());
        chapter.insert("pageSize".into(), page_size.to_string());
        // 前置链(捕获 token 等)在主请求前跑一次。
        self.run_prelude(&op.prelude, &mut chapter).await?;

        let vars = self.flatten(&chapter);
        // 完整响应:body(列表/has_more 等)+ 可选渲染 DOM(via:css 的 totalPages,见 render-dual-source)。
        let resp = self
            .send_templated_full(
                &op.request.url,
                op.request.method,
                op.request.body.as_ref(),
                &op.request.headers,
                &vars,
                // 点击驱动翻页(search-click-pagination):URL 不认页码的 SPA(番茄 search)靠
                // pageBy.click 在一张活页点 page-1 次翻到目标页;page_by 缺席 = 现状单页。
                RenderArgs {
                    render: op.request.render,
                    ready_for: op.request.ready_for.as_deref(),
                    intercept_api: op.request.intercept_api.as_deref(),
                    page,
                    page_by: op.request.page_by.as_ref().map(|p| p.click.as_str()),
                },
            )
            .await?;
        let html = &resp.body;
        // 主请求 vars 捕获(chapter 级):对搜索响应求值,使 list/item 可见(captured-before-referenced)。
        // flatten 刻意分两次:各条 vars **独立**对响应求值(见 source `Request.vars` 契约「勿互相引用」,
        // 有序依赖应走 prelude 链)。
        let flat = self.flatten(&chapter);
        for (name, rule) in &op.request.vars {
            let v = eval_value(rule, html, &flat)?;
            if !v.is_empty() {
                chapter.insert(name.clone(), v);
            }
        }
        let vars = self.flatten(&chapter);
        let items = self.eval_list_items(&op.list, &op.item, html, &vars)?;
        let dom = resp.dom_html.as_deref();
        let total_pages = self.eval_total_pages(op.request.total_pages.as_ref(), html, dom, &vars);
        let has_more = self.eval_has_more(op.request.has_more.as_ref(), html, dom, &vars);
        let result = BookList {
            items,
            total_pages,
            has_more,
        };
        if let Some(k) = cache_key {
            self.cache_page(k, &result);
        }
        Ok(result)
    }

    /// 浏览某分类的某一页(可选渲染取页;由用户递增 `page` 单页取)。返回书列表 + 可选总页数。
    pub async fn explore(
        &self,
        category_url: &UrlOrRule,
        page: u32,
        page_size: u32,
    ) -> Result<BookList> {
        let op = self
            .source
            .explore
            .as_ref()
            .ok_or(BookSourceError::Missing("explore"))?;
        // 渲染结果缓存(仅 render):键用**静态分类 URL 模板** + 页(模板各分类不同、`{{page}}` 仍是
        // 字面量,page 单独入键 → 各分类各页唯一)。`Rule` 形分类 URL 无静态键 → 不缓存。
        let cache_key = match (op.render, category_url) {
            (true, UrlOrRule::Str(tpl)) => Some(format!("e\u{0}{tpl}\u{0}{page}\u{0}{page_size}")),
            _ => None,
        };
        if let Some(hit) = cache_key.as_deref().and_then(|k| self.cached_page(k)) {
            return Ok(hit);
        }
        let mut chapter = self.base_vars();
        chapter.insert("page".into(), page.to_string());
        chapter.insert("pageSize".into(), page_size.to_string());
        self.run_prelude(&op.prelude, &mut chapter).await?;
        let vars = self.flatten(&chapter);
        let resp = if op.render {
            // 渲染取页:与 search 主请求同款路由(send_templated_full → run_request_full → fetch_full)。
            // 空 headers:explore 分类请求无自定义头,渲染配置经 op 承载(对齐 search 主请求路由)。
            let no_headers = std::collections::HashMap::new();
            self.send_templated_full(
                category_url,
                Method::Get,
                None,
                &no_headers,
                &vars,
                // explore 是 URL 驱动(/library/all/page_{{page}}),不点击翻页 → page=0/page_by=None。
                RenderArgs {
                    render: op.render,
                    ready_for: op.ready_for.as_deref(),
                    intercept_api: op.intercept_api.as_deref(),
                    ..Default::default()
                },
            )
            .await?
        } else {
            // 未开 render:reqwest 直取(现状,逐字节不变);走 run_request_full 以便 via:css 的
            // totalPages 也能对整页 HTML 求值(便宜档,无 DOM 也有总页数源)。
            let url = self.resolve_url(category_url, &vars)?;
            self.run_request_full(self.get_req(url)).await?
        };
        let html = &resp.body;
        let items = self.eval_list_items(&op.list, &op.item, html, &vars)?;
        let dom = resp.dom_html.as_deref();
        let total_pages = self.eval_total_pages(op.total_pages.as_ref(), html, dom, &vars);
        let has_more = self.eval_has_more(op.has_more.as_ref(), html, dom, &vars);
        let result = BookList {
            items,
            total_pages,
            has_more,
        };
        if let Some(k) = cache_key {
            self.cache_page(k, &result);
        }
        Ok(result)
    }

    /// 浏览分类列表,供上层选择后翻页。
    pub fn explore_categories(&self) -> Vec<Category> {
        self.source
            .explore
            .as_ref()
            .map(|e| e.categories.clone())
            .unwrap_or_default()
    }
}
