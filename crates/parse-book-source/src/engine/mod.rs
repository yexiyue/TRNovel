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
use crate::model::{BookInfo, BookList, Chapter, ExploreEntry, Toc, Volume};
use crate::source::BookSource;
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
    /// explore 动态入口加载缓存(`dynamic-explore-entries`):入口源可能请求远端分类 API,
    /// 缓存一次成功加载的 `Vec<ExploreEntry>`,避免 UI 每次进入 explore 都重抓。随 `Clone` 的引擎
    /// 共享(`Arc`),per-source 会话级(引擎重建即清空)。**仅完全成功(无动态源报错)才缓存**:
    /// 有动态源失败时返回已成功的部分入口但不缓存,使下次进入可重试。
    entries_cache: Arc<RwLock<Option<Vec<ExploreEntry>>>>,
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
            entries_cache: Arc::new(RwLock::new(None)),
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

    /// 搜索。返回书列表 + 可选精确总页数(`render-dual-source`)/ 边界(`list-has-more`)。
    /// `key` 作为 `{{key}}` 变量并入共享列表页 runner。
    pub async fn search(&self, key: &str, page: u32, page_size: u32) -> Result<BookList> {
        let op = self
            .source
            .search
            .as_ref()
            .ok_or(BookSourceError::Missing("search"))?;
        let mut extra = BTreeMap::new();
        extra.insert("key".to_string(), key.to_string());
        self.run_list_page(op, 's', &extra, page, page_size).await
    }

    /// 浏览选中入口的某一页(由用户递增 `page` 单页取)。用入口变量驱动共享列表页 runner;
    /// 取页 URL 由 `explore.page.request` 用入口变量 + `{{page}}` 模板生成(入口不再持固定 URL)。
    pub async fn explore(
        &self,
        entry: &ExploreEntry,
        page: u32,
        page_size: u32,
    ) -> Result<BookList> {
        let op = self
            .source
            .explore
            .as_ref()
            .ok_or(BookSourceError::Missing("explore"))?;
        self.run_list_page(&op.page, 'e', &entry.vars, page, page_size)
            .await
    }

    /// 加载 explore 入口(`dynamic-explore-entries`):按声明顺序遍历入口源数组,合并各源产出的
    /// 扁平 `ExploreEntry`(静态固定入口 + 远端抓取动态入口)。
    ///
    /// 失败策略:某个(动态)源失败时保留已成功入口(含前面的静态源),不阻断;仅当**零入口**产出
    /// 且有源报错时返回该错误。完全成功(无任何源报错)才写入口缓存,使下次进入可重试失败的动态源。
    pub async fn explore_entries(&self) -> Result<Vec<ExploreEntry>> {
        if let Some(cached) = self.entries_cache.read().ok().and_then(|g| g.clone()) {
            return Ok(cached);
        }
        let Some(op) = self.source.explore.as_ref() else {
            return Ok(Vec::new());
        };
        let mut out: Vec<ExploreEntry> = Vec::new();
        let mut first_err: Option<BookSourceError> = None;
        for src in &op.entries {
            match self.load_entry_source(src).await {
                Ok(mut entries) => out.append(&mut entries),
                Err(e) => {
                    first_err.get_or_insert(e);
                }
            }
        }
        // 零入口且有报错 → 把错误冒泡给 UI(否则用户看到空分类却无原因)。
        if out.is_empty()
            && let Some(e) = first_err.take()
        {
            return Err(e);
        }
        // 完全成功才缓存;有动态源失败时返回部分入口但不缓存(下次进入重试)。
        if first_err.is_none()
            && let Ok(mut g) = self.entries_cache.write()
        {
            *g = Some(out.clone());
        }
        Ok(out)
    }
}
