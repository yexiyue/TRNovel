//! 用例引擎(Template Method + Paginator)。五个操作共享「取页 → 选列表/值 → 映射 →
//! 可选有界分页」骨架;`Engine` 廉价 `Clone`(内部 `Arc`),操作不跨 await 持锁(D10)。

use super::cookie::{
    CookieJar, merge_login_into_headers, registrable_domain, request_registrable_domain,
};
use super::error::{BookSourceError, Result};
use super::eval::{Vars, eval_list, eval_value, interpolate};
use super::fetch::{FetchRequest, Fetcher, ReqwestFetcher};
use super::model::{BookInfo, BookListItem, Chapter, Toc, Volume};
use super::source::{
    BookRules, BookSource, Capture, Category, Method, PreStep, Rule, UrlOrRule, VarScope,
};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, RwLock};

/// 书源运行时引擎。
#[derive(Clone)]
pub struct Engine {
    source: Arc<BookSource>,
    fetcher: Arc<dyn Fetcher>,
    /// 登录态请求头(JWT/自定义头/Cookie 同路径),并入引擎构造的每个**同注册域**请求
    /// (跨注册域请求跳过,防页面内容诱导的第三方 URL 外泄凭据,见 [`merge_login_into_headers`])。
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
        browser: Option<crate::browser::BrowserFetcher>,
    ) -> Result<Self> {
        let fetcher = crate::browser::EscalatingFetcher::new(&source, browser)?;
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

    fn base_vars(&self) -> Vars {
        let mut v = Vars::new();
        v.insert(
            "base".into(),
            self.source.url.trim_end_matches('/').to_string(),
        );
        v
    }

    /// 构造一个带登录态的 GET 请求——引擎所有取页统一经此并入 loginHeader + cookie 库。
    fn get_req(&self, url: impl Into<String>) -> FetchRequest {
        let mut req = FetchRequest::get(url);
        let url = req.url.clone();
        self.apply_auth(&url, &mut req.headers);
        req
    }

    /// 注册域(请求 URL 绝对则取其注册域,相对则取书源注册域)。
    fn request_domain(&self, url: &str) -> String {
        request_registrable_domain(url, &registrable_domain(&self.source.url))
    }

    /// 把登录态并入请求头(合并的最后一层),与 host 侧共用 [`merge_login_into_headers`]:
    /// loginHeader 仅注入**同注册域**请求(防页面内容诱导的第三方 URL 外泄凭据);
    /// Cookie = 已有头 Cookie ← loginHeader Cookie ← cookie 库(请求注册域)按 key 去重合并;
    /// 全部值剥 CR/LF(已落盘的脏数据不致让 reqwest 构建失败、拖垮该书源全部请求)。
    fn apply_auth(&self, url: &str, headers: &mut HashMap<String, String>) {
        let source_domain = registrable_domain(&self.source.url);
        let domain = request_registrable_domain(url, &source_domain);
        let jar_cookie = self
            .cookies
            .read()
            .ok()
            .and_then(|j| j.cookie_header(&domain));
        merge_login_into_headers(
            &self.login_header,
            &source_domain,
            &domain,
            jar_cookie.as_deref(),
            headers,
        );
    }

    /// 发请求(带登录态)→ `enabledCookieJar` 时回灌 `Set-Cookie` → `loginCheckJs` 校验登录态。
    /// 失效返回 [`BookSourceError::LoginExpired`]。引擎所有取页统一经此。
    async fn run_request(&self, req: FetchRequest) -> Result<String> {
        let domain = self.request_domain(&req.url);
        // 渲染取页(render-fetcher):body 是拦截的 API 响应 / 渲染后 DOM(非整页),且无真实响应头
        // (浏览器渲染响应不回传 Set-Cookie)。故不参与 loginCheckJs 整页校验,避免误判登录失效。
        let is_render = req.render;
        let resp = self.fetcher.fetch_full(req).await?;
        if self.source.enabled_cookie_jar
            && let Some(set_cookie) = resp.headers.get("set-cookie")
            && let Ok(mut jar) = self.cookies.write()
        {
            jar.absorb_set_cookie(&domain, set_cookie);
        }
        if !is_render {
            self.check_login(&resp.body)?;
        }
        Ok(resp.body)
    }

    /// 取页(带登录态 + 回灌 + 登录校验)。
    async fn fetch_checked(&self, url: impl Into<String>) -> Result<String> {
        self.run_request(self.get_req(url)).await
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

    /// 搜索。
    pub async fn search(&self, key: &str, page: u32, page_size: u32) -> Result<Vec<BookListItem>> {
        let op = self
            .source
            .search
            .as_ref()
            .ok_or(BookSourceError::Missing("search"))?;
        let mut chapter = self.base_vars();
        chapter.insert("key".into(), key.to_string());
        chapter.insert("page".into(), page.to_string());
        chapter.insert("pageSize".into(), page_size.to_string());
        self.run_prelude(&op.prelude, &mut chapter).await?;

        let vars = self.flatten(&chapter);
        let html = self
            .send_templated(
                &op.request.url,
                op.request.method,
                op.request.body.as_ref(),
                &op.request.headers,
                &vars,
                op.request.render,
                op.request.ready_for.as_deref(),
                op.request.intercept_api.as_deref(),
            )
            .await?;
        // 主请求 vars 捕获(chapter 级):对搜索响应求值,使 list/item 可见(captured-before-referenced)。
        // flatten 刻意提在循环外:各条 vars **独立**对响应求值(见 source.rs `Request.vars` 契约
        // 「勿互相引用」,有序依赖应走 prelude 链),也避免循环内重复克隆三层作用域。
        let flat = self.flatten(&chapter);
        for (name, rule) in &op.request.vars {
            let v = eval_value(rule, &html, &flat)?;
            if !v.is_empty() {
                chapter.insert(name.clone(), v);
            }
        }
        self.eval_list_items(&op.list, &op.item, &html, &self.flatten(&chapter))
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
        let mut chapter = self.base_vars();
        chapter.insert("page".into(), page.to_string());
        chapter.insert("pageSize".into(), page_size.to_string());
        self.run_prelude(&op.prelude, &mut chapter).await?;
        let vars = self.flatten(&chapter);
        let url = self.resolve_url(category_url, &vars)?;
        let html = self.fetch_checked(url).await?;
        self.eval_list_items(&op.list, &op.item, &html, &vars)
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

    /// 把 chapter 层与引擎的 book/source 层 overlay 成单个扁平 `Vars`(`interpolate` 只吃扁平表)。
    /// 优先级 `source < book < chapter`(高优先级后插覆盖)= get 时 章节→书籍→书源 取第一个非空。
    fn flatten(&self, chapter: &Vars) -> Vars {
        let mut out = Vars::new();
        if let Ok(g) = self.source_vars.read() {
            out.extend(g.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        if let Ok(g) = self.book_vars.read() {
            out.extend(g.iter().map(|(k, v)| (k.clone(), v.clone())));
        }
        out.extend(chapter.iter().map(|(k, v)| (k.clone(), v.clone())));
        out
    }

    /// 执行前置请求链(D7-bis):按数组顺序串行发请求,每步对其响应做命名捕获写入作用域。
    /// `chapter` 是本次调用的临时层(含 base/key/page),chapter 级捕获就地累积;捕获天然先于引用
    /// (响应后才捕获 + 数组顺序)。锁仅在求值前后瞬时持有,不跨 await(满足 D10)。
    async fn run_prelude(&self, steps: &[PreStep], chapter: &mut Vars) -> Result<()> {
        for step in steps {
            // skipIfPresent:列出的 key 在作用域内全部非空 → 跳过本步(token 复用,省 RTT)。
            if !step.skip_if_present.is_empty() {
                let flat = self.flatten(chapter);
                if step
                    .skip_if_present
                    .iter()
                    .all(|k| flat.get(k).is_some_and(|v| !v.is_empty()))
                {
                    continue;
                }
            }
            let flat = self.flatten(chapter);
            let resp = self
                .send_templated(
                    &step.url,
                    step.method,
                    step.body.as_ref(),
                    &step.headers,
                    &flat,
                    false,
                    None,
                    None,
                )
                .await?;
            self.capture_into(&step.capture, &resp, chapter)?;
        }
        Ok(())
    }

    /// 发送一个「模板化请求」——search 主请求与 prelude 步骤共用的五步骨架:
    /// resolve url/body → header 值 `{{name}}` 插值(可引用前置捕获的 token)→
    /// 并入登录态(apply_auth)→ [`Engine::run_request`]。
    /// `vars` 须为调用方已 flatten 的扁平表;请求后的差异化处理(`Request.vars` 捕获 /
    /// prelude 的 `capture_into`)留在调用点。
    #[allow(clippy::too_many_arguments)]
    async fn send_templated(
        &self,
        url: &UrlOrRule,
        method: Method,
        body: Option<&UrlOrRule>,
        headers: &HashMap<String, String>,
        vars: &Vars,
        // 渲染型取页配置(`render-fetcher`):prelude 等普通请求传 `false, None, None`;
        // search 等可渲染的 op 传其 `request.render/ready_for/intercept_api`。
        render: bool,
        ready_for: Option<&str>,
        intercept_api: Option<&str>,
    ) -> Result<String> {
        let url = self.resolve_url(url, vars)?;
        let body = match body {
            Some(b) => Some(self.resolve_url(b, vars)?),
            None => None,
        };
        let mut hdrs = HashMap::with_capacity(headers.len());
        for (k, v) in headers {
            hdrs.insert(k.clone(), interpolate(v, vars));
        }
        self.apply_auth(&url, &mut hdrs);
        self.run_request(FetchRequest {
            url,
            method,
            body,
            headers: hdrs,
            render,
            ready_for: ready_for.map(str::to_string),
            intercept_api: intercept_api.map(str::to_string),
        })
        .await
    }

    /// 对一段响应按 `capture` 顺序求值并写入各作用域层;空串不写(防污染低优先级层的非空值)。
    fn capture_into(&self, caps: &[Capture], body: &str, chapter: &mut Vars) -> Result<()> {
        for cap in caps {
            let v = eval_value(&cap.value, body, &self.flatten(chapter))?;
            if v.is_empty() {
                continue;
            }
            match cap.scope {
                VarScope::Chapter => {
                    chapter.insert(cap.name.clone(), v);
                }
                VarScope::Book => {
                    if let Ok(mut g) = self.book_vars.write() {
                        g.insert(cap.name.clone(), v);
                    }
                }
                VarScope::Source => {
                    if let Ok(mut g) = self.source_vars.write() {
                        g.insert(cap.name.clone(), v);
                    }
                }
            }
        }
        Ok(())
    }

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
        vars: &Vars,
    ) -> Result<Vec<BookListItem>> {
        let mut out = Vec::new();
        for ctx in eval_list(list, html)? {
            let info = self.eval_book_info(item, &ctx, vars)?;
            let book_url = opt_eval(item.book_url.as_ref(), &ctx, vars)?;
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
    use crate::fetch::{FetchResponse, Fetcher};
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

    /// 记录请求 Cookie 头并固定返回一个 `Set-Cookie` 的替身(验证 enabledCookieJar 回灌/再发)。
    struct CookieEchoFetcher {
        set_cookie: String,
        last_cookie: Arc<Mutex<Option<String>>>,
    }

    #[async_trait]
    impl Fetcher for CookieEchoFetcher {
        async fn fetch(&self, req: FetchRequest) -> std::result::Result<String, FetchError> {
            self.fetch_full(req).await.map(|r| r.body)
        }
        async fn fetch_full(
            &self,
            req: FetchRequest,
        ) -> std::result::Result<FetchResponse, FetchError> {
            *self.last_cookie.lock().unwrap() = req.headers.get("Cookie").cloned();
            let mut headers = HashMap::new();
            headers.insert("set-cookie".to_string(), self.set_cookie.clone());
            Ok(FetchResponse {
                body: CATALOG.to_string(),
                status: 200,
                headers,
            })
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

    // ── 审查/security:loginHeader 仅注入同注册域请求(页面内容诱导的第三方 URL 不外泄凭据)──
    #[tokio::test]
    async fn login_header_not_sent_to_other_registrable_domain() {
        let src = BookSource::from_json(SOURCE).unwrap(); // 书源 url "https://x"
        let captured = Arc::new(Mutex::new(HashMap::new()));
        let fetcher = Arc::new(RecordingFetcher {
            body: CATALOG.to_string(),
            last_headers: captured.clone(),
        });
        let mut lh = BTreeMap::new();
        lh.insert("Authorization".into(), "Bearer T".into());
        lh.insert("Cookie".into(), "sid=1".into());
        let engine = Engine::with_fetcher(src, fetcher).with_login_header(lh);
        // 绝对 URL 指向其它注册域(模拟被挂马页面把 toc/next_page 指向第三方域)。
        engine.toc("https://evil.example.org/any").await.unwrap();
        let h = captured.lock().unwrap();
        assert!(
            h.get("Authorization").is_none(),
            "跨注册域不应携带登录头: {h:?}"
        );
        assert!(
            h.get("Cookie").is_none(),
            "跨注册域不应携带登录 Cookie: {h:?}"
        );
    }

    // ── 审查/correctness:含 \n 的 loginHeader(脏落盘数据)经 apply_auth 剥除,引擎请求可送出 ──
    #[tokio::test]
    async fn newline_in_login_header_sanitized_in_engine_requests() {
        let src = BookSource::from_json(SOURCE).unwrap();
        let captured = Arc::new(Mutex::new(HashMap::new()));
        let fetcher = Arc::new(RecordingFetcher {
            body: CATALOG.to_string(),
            last_headers: captured.clone(),
        });
        let mut lh = BTreeMap::new();
        // 模拟脚本把 \n 连接的多 Set-Cookie 直接写回 Cookie 后落盘的脏数据。
        lh.insert("Cookie".into(), "a=1\nb=2".into());
        let engine = Engine::with_fetcher(src, fetcher).with_login_header(lh);
        engine.toc("/any").await.unwrap();
        let h = captured.lock().unwrap();
        let cookie = h.get("Cookie").cloned().unwrap_or_default();
        assert!(!cookie.contains('\n'), "Cookie 的 \\n 应被剥除: {cookie:?}");
        assert_eq!(cookie, "a=1b=2", "与 host 侧 sanitize 行为对称");
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

    // ── 10.2/10.3/10.6:enabledCookieJar 回灌 Set-Cookie → 后续请求携带 → persistent 导出 ──
    #[tokio::test]
    async fn enabled_cookie_jar_absorbs_resends_and_persists() {
        let json = SOURCE.replacen(
            "\"bookInfo\":{}",
            "\"enabledCookieJar\":true,\"bookInfo\":{}",
            1,
        );
        let src = BookSource::from_json(&json).unwrap();
        let last = Arc::new(Mutex::new(None));
        let fetcher = Arc::new(CookieEchoFetcher {
            set_cookie: "token=xyz; Max-Age=3600; Path=/".to_string(),
            last_cookie: last.clone(),
        });
        let engine = Engine::with_fetcher(src, fetcher);

        // 首请求:无 cookie 发出,响应 Set-Cookie 被回灌。
        engine.toc("/p1").await.unwrap();
        assert!(last.lock().unwrap().is_none(), "首请求不应带 cookie");
        // 后续请求:回灌的 token 随请求发出。
        engine.book_info("/p2").await.unwrap();
        assert_eq!(
            last.lock().unwrap().clone(),
            Some("token=xyz".to_string()),
            "回灌 cookie 应随后续请求发出"
        );
        // persistent 导出含 token(Max-Age → persistent),供 app 落盘。
        // 书源 url "https://x" 的注册域为 "x"。
        assert_eq!(
            engine.persistent_cookies().get("x").map(String::as_str),
            Some("token=xyz")
        );
    }

    // ── 审查/correctness:warmup 走统一 run_request,enabledCookieJar 时预热页 Set-Cookie 回灌 ──
    #[tokio::test]
    async fn warmup_absorbs_set_cookie_into_jar() {
        let json = SOURCE.replacen(
            "\"bookInfo\":{}",
            "\"enabledCookieJar\":true,\"http\":{\"warmup\":[\"https://x/warm\"]},\"bookInfo\":{}",
            1,
        );
        let src = BookSource::from_json(&json).unwrap();
        let last = Arc::new(Mutex::new(None));
        let fetcher = Arc::new(CookieEchoFetcher {
            set_cookie: "token=warm; Max-Age=3600; Path=/".to_string(),
            last_cookie: last.clone(),
        });
        let engine = Engine::with_fetcher(src, fetcher);
        engine.warmup().await;
        // 预热页种下的 cookie 应进引擎 CookieJar(persistent 可导出落盘 / net.getCookie 可见)。
        assert_eq!(
            engine.persistent_cookies().get("x").map(String::as_str),
            Some("token=warm"),
            "预热页的 Set-Cookie 应回灌引擎 cookie 库"
        );
    }

    // 未开 enabledCookieJar 时不回灌(向后兼容)。
    #[tokio::test]
    async fn cookie_jar_disabled_does_not_absorb() {
        let src = BookSource::from_json(SOURCE).unwrap();
        let last = Arc::new(Mutex::new(None));
        let fetcher = Arc::new(CookieEchoFetcher {
            set_cookie: "token=xyz; Max-Age=3600".to_string(),
            last_cookie: last.clone(),
        });
        let engine = Engine::with_fetcher(src, fetcher);
        engine.toc("/p1").await.unwrap();
        engine.book_info("/p2").await.unwrap();
        assert!(
            last.lock().unwrap().is_none(),
            "未开 cookieJar 不应回灌/再发"
        );
        assert!(engine.persistent_cookies().is_empty());
    }

    // ───────────────────── 11.x:前置请求链 / 结构化捕获 ─────────────────────

    /// 按 URL 子串路由到不同响应体的替身(模拟前置链:prepare → 主请求),并记录调用 URL。
    struct ScriptedFetcher {
        routes: Vec<(String, String)>,
        calls: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl Fetcher for ScriptedFetcher {
        async fn fetch(&self, req: FetchRequest) -> std::result::Result<String, FetchError> {
            self.calls.lock().unwrap().push(req.url.clone());
            for (pat, body) in &self.routes {
                if req.url.contains(pat.as_str()) {
                    return Ok(body.clone());
                }
            }
            Ok(String::new())
        }
    }

    fn scripted(routes: Vec<(&str, &str)>) -> (Arc<ScriptedFetcher>, Arc<Mutex<Vec<String>>>) {
        let calls = Arc::new(Mutex::new(Vec::new()));
        let f = Arc::new(ScriptedFetcher {
            routes: routes
                .into_iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            calls: calls.clone(),
        });
        (f, calls)
    }

    // 前置请求链捕获 token → 带入主搜索请求 URL(headline:本 op 内多步)。
    #[tokio::test]
    async fn prelude_captures_token_into_main_request() {
        let json = r#"{
          "schema":"trnovel-booksource/v2","name":"t","url":"https://x",
          "search":{
            "prelude":[{"url":{"template":"{{base}}/prepare"},
              "capture":[{"name":"token","value":{"via":"raw","clean":[{"trim":true}]},"scope":"chapter"}]}],
            "request":{"url":{"template":"{{base}}/search?kw={{key}}&token={{token}}"}},
            "list":{"via":"css","select":".item"},
            "item":{"name":{"via":"css","select":".t","extract":"text"}}
          },
          "bookInfo":{},
          "toc":{"list":{"via":"css","select":"a"},"name":{"via":"css","select":"a"},"url":{"via":"css","select":"a","extract":{"attr":"href"}}},
          "content":{"value":{"via":"css","select":".c"}}
        }"#;
        let src = BookSource::from_json(json).unwrap();
        let (f, calls) = scripted(vec![
            ("/prepare", "ABC"),
            (
                "/search",
                r#"<div class="item"><span class="t">书名</span></div>"#,
            ),
        ]);
        let engine = Engine::with_fetcher(src, f);
        let items = engine.search("k", 1, 20).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].info.name, "书名");
        let c = calls.lock().unwrap();
        assert!(
            c.iter().any(|u| u.contains("/prepare")),
            "应先跑前置 prepare: {c:?}"
        );
        assert!(
            c.iter().any(|u| u.contains("token=ABC")),
            "主搜索应带捕获的 token: {c:?}"
        );
    }

    // source 作用域 + skipIfPresent:同一引擎跨调用复用 token,prepare 只跑一次。
    #[tokio::test]
    async fn skip_if_present_reuses_source_scope_token() {
        let json = r#"{
          "schema":"trnovel-booksource/v2","name":"t","url":"https://x",
          "search":{
            "prelude":[{"url":{"template":"{{base}}/prepare"},
              "capture":[{"name":"token","value":{"via":"raw","clean":[{"trim":true}]},"scope":"source"}],
              "skipIfPresent":["token"]}],
            "request":{"url":{"template":"{{base}}/search?token={{token}}"}},
            "list":{"via":"css","select":".item"},
            "item":{"name":{"via":"css","select":".t","extract":"text"}}
          },
          "bookInfo":{},
          "toc":{"list":{"via":"css","select":"a"},"name":{"via":"css","select":"a"},"url":{"via":"css","select":"a","extract":{"attr":"href"}}},
          "content":{"value":{"via":"css","select":".c"}}
        }"#;
        let src = BookSource::from_json(json).unwrap();
        let (f, calls) = scripted(vec![
            ("/prepare", "TKN"),
            (
                "/search",
                r#"<div class="item"><span class="t">x</span></div>"#,
            ),
        ]);
        let engine = Engine::with_fetcher(src, f);
        engine.search("a", 1, 20).await.unwrap();
        engine.search("b", 1, 20).await.unwrap();
        let prepares = calls
            .lock()
            .unwrap()
            .iter()
            .filter(|u| u.contains("/prepare"))
            .count();
        assert_eq!(
            prepares, 1,
            "skipIfPresent 应使 source 级 token 复用,prepare 只跑一次"
        );
        assert_eq!(
            engine.source_vars().get("token").map(String::as_str),
            Some("TKN")
        );
    }

    // 主请求 vars 捕获对 list/item 可见(自捕获边界:响应后才可见)。
    #[tokio::test]
    async fn request_vars_visible_to_list_items() {
        let json = r#"{
          "schema":"trnovel-booksource/v2","name":"t","url":"https://x",
          "search":{
            "request":{"url":{"template":"{{base}}/s"},
              "vars":{"site":{"via":"css","select":".site","extract":"text"}}},
            "list":{"via":"css","select":".item"},
            "item":{"name":{"template":"{{site}}-书"}}
          },
          "bookInfo":{},
          "toc":{"list":{"via":"css","select":"a"},"name":{"via":"css","select":"a"},"url":{"via":"css","select":"a","extract":{"attr":"href"}}},
          "content":{"value":{"via":"css","select":".c"}}
        }"#;
        let src = BookSource::from_json(json).unwrap();
        let html = r#"<span class="site">甲站</span><div class="item">x</div>"#;
        let engine = Engine::with_fetcher(src, Arc::new(MockFetcher(html.to_string())));
        let items = engine.search("k", 1, 20).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(
            items[0].info.name, "甲站-书",
            "item 模板应看到主请求捕获的 site"
        );
    }

    // 空串捕获不写作用域层(抽取失败 → {{x}} 落空串,不污染)。
    #[tokio::test]
    async fn empty_capture_not_written() {
        let json = r#"{
          "schema":"trnovel-booksource/v2","name":"t","url":"https://x",
          "search":{
            "prelude":[{"url":{"template":"{{base}}/p"},
              "capture":[{"name":"x","value":{"via":"css","select":".nope","extract":"text"},"scope":"source"}]}],
            "request":{"url":{"template":"{{base}}/s?x={{x}}"}},
            "list":{"via":"css","select":".item"},
            "item":{"name":{"via":"css","select":".t","extract":"text"}}
          },
          "bookInfo":{},
          "toc":{"list":{"via":"css","select":"a"},"name":{"via":"css","select":"a"},"url":{"via":"css","select":"a","extract":{"attr":"href"}}},
          "content":{"value":{"via":"css","select":".c"}}
        }"#;
        let src = BookSource::from_json(json).unwrap();
        let (f, calls) = scripted(vec![
            ("/p", "<html></html>"),
            ("/s", r#"<div class="item"><span class="t">y</span></div>"#),
        ]);
        let engine = Engine::with_fetcher(src, f);
        engine.search("k", 1, 20).await.unwrap();
        assert!(
            !engine.source_vars().contains_key("x"),
            "空串捕获不应写作用域层"
        );
        assert!(
            calls.lock().unwrap().iter().any(|u| u.contains("/s?x=")),
            "主请求应照常发出(x 为空串)"
        );
    }

    // toc 前置 csrf → 目录抽取规则(concat)引用 {{csrf}}(headline:前置链 + 抽取可见捕获)。
    #[tokio::test]
    async fn toc_prelude_csrf_visible_to_extraction() {
        let json = r#"{
          "schema":"trnovel-booksource/v2","name":"t","url":"https://x",
          "bookInfo":{},
          "toc":{
            "prelude":[{"url":{"template":"{{base}}/prepare"},
              "capture":[{"name":"csrf","value":{"via":"raw","clean":[{"trim":true}]},"scope":"chapter"}]}],
            "list":{"via":"css","select":".ch"},
            "name":{"via":"css","select":"a","extract":"text"},
            "url":{"concat":[{"literal":"/c?sign="},{"template":"{{csrf}}"},{"literal":"&href="},{"via":"css","select":"a","extract":{"attr":"href"}}]},
            "maxPages":1
          },
          "content":{"value":{"via":"css","select":".c"}}
        }"#;
        let src = BookSource::from_json(json).unwrap();
        let (f, _calls) = scripted(vec![
            ("/prepare", "SIG"),
            (
                "/toc",
                r#"<div class="ch"><a href="/n/1.html">第一章</a></div>"#,
            ),
        ]);
        let engine = Engine::with_fetcher(src, f);
        let toc = engine.toc("/toc/1").await.unwrap();
        assert_eq!(toc.chapters.len(), 1);
        assert_eq!(
            toc.chapters[0].url, "/c?sign=SIG&href=/n/1.html",
            "目录 url 应拼入前置捕获的 csrf"
        );
    }

    // 审查 fix1:主请求 header 值支持 {{name}} 模板,可引用前置捕获的 token。
    #[tokio::test]
    async fn main_request_headers_interpolate_captured_vars() {
        let json = r#"{
          "schema":"trnovel-booksource/v2","name":"t","url":"https://x",
          "search":{
            "prelude":[{"url":{"template":"{{base}}/prepare"},
              "capture":[{"name":"token","value":{"via":"raw","clean":[{"trim":true}]},"scope":"chapter"}]}],
            "request":{"url":{"template":"{{base}}/search"},
              "headers":{"Authorization":"Bearer {{token}}"}},
            "list":{"via":"css","select":".item"},
            "item":{"name":{"via":"css","select":".t","extract":"text"}}
          },
          "bookInfo":{},
          "toc":{"list":{"via":"css","select":"a"},"name":{"via":"css","select":"a"},"url":{"via":"css","select":"a","extract":{"attr":"href"}}},
          "content":{"value":{"via":"css","select":".c"}}
        }"#;
        let src = BookSource::from_json(json).unwrap();
        let seen = Arc::new(Mutex::new(None));
        struct HeaderProbe {
            seen: Arc<Mutex<Option<String>>>,
        }
        #[async_trait]
        impl Fetcher for HeaderProbe {
            async fn fetch(&self, req: FetchRequest) -> std::result::Result<String, FetchError> {
                if req.url.contains("/search") {
                    *self.seen.lock().unwrap() = req.headers.get("Authorization").cloned();
                    return Ok(r#"<div class="item"><span class="t">书</span></div>"#.to_string());
                }
                Ok("ABC".to_string()) // /prepare
            }
        }
        let engine = Engine::with_fetcher(src, Arc::new(HeaderProbe { seen: seen.clone() }));
        engine.search("k", 1, 20).await.unwrap();
        assert_eq!(
            seen.lock().unwrap().clone(),
            Some("Bearer ABC".to_string()),
            "主请求 header 应插值前置捕获的 token"
        );
    }

    // 审查 fix2:多条 Request.vars 都被捕获且对 item 可见(BTreeMap 确定序)。
    #[tokio::test]
    async fn multiple_request_vars_all_captured() {
        let json = r#"{
          "schema":"trnovel-booksource/v2","name":"t","url":"https://x",
          "search":{
            "request":{"url":{"template":"{{base}}/s"},
              "vars":{
                "a":{"via":"css","select":".a","extract":"text"},
                "b":{"via":"css","select":".b","extract":"text"}
              }},
            "list":{"via":"css","select":".item"},
            "item":{"name":{"template":"{{a}}-{{b}}"}}
          },
          "bookInfo":{},
          "toc":{"list":{"via":"css","select":"a"},"name":{"via":"css","select":"a"},"url":{"via":"css","select":"a","extract":{"attr":"href"}}},
          "content":{"value":{"via":"css","select":".c"}}
        }"#;
        let src = BookSource::from_json(json).unwrap();
        let html = r#"<span class="a">甲</span><span class="b">乙</span><div class="item">x</div>"#;
        let engine = Engine::with_fetcher(src, Arc::new(MockFetcher(html.to_string())));
        let items = engine.search("k", 1, 20).await.unwrap();
        assert_eq!(
            items[0].info.name, "甲-乙",
            "多条 request.vars 应都被捕获且对 item 可见"
        );
    }
}
