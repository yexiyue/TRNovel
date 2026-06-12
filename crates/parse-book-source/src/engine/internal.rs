//! 引擎内部管线(私有 `impl Engine`):请求构造与登录态注入、取页与登录校验、模板化请求
//! 骨架、命名捕获、有界分页、列表/详情求值。均为 [`super::Engine`] 五个公开操作共享的实现细节。

use super::{Engine, RenderArgs};
use crate::error::{BookSourceError, Result};
use crate::eval::{Vars, eval_list, eval_value, interpolate};
use crate::fetch::cookie::{
    merge_login_into_headers, registrable_domain, request_registrable_domain,
};
use crate::fetch::{FetchRequest, FetchResponse};
use crate::model::{BookInfo, BookList, BookListItem, ExploreEntry};
use crate::source::{
    BookRules, Capture, EntrySource, FetchEntrySource, ListPageSpec, Method, PreStep, Rule,
    UrlOrRule, VarScope,
};
use std::collections::{BTreeMap, HashMap};

impl Engine {
    pub(super) fn base_vars(&self) -> Vars {
        let mut v = Vars::new();
        v.insert(
            "base".into(),
            self.source.url.trim_end_matches('/').to_string(),
        );
        v
    }

    /// 构造一个带登录态的 GET 请求——引擎所有取页统一经此并入 loginHeader + cookie 库。
    pub(super) fn get_req(&self, url: impl Into<String>) -> FetchRequest {
        let mut req = FetchRequest::get(url);
        let url = req.url.clone();
        self.apply_auth(&url, &mut req.headers);
        req
    }

    /// 注册域(请求 URL 绝对则取其注册域,相对则取书源注册域)。
    pub(super) fn request_domain(&self, url: &str) -> String {
        request_registrable_domain(url, &registrable_domain(&self.source.url))
    }

    /// 把登录态并入请求头(合并的最后一层),与 host 侧共用 [`merge_login_into_headers`]:
    /// loginHeader 仅注入**同注册域**请求(防页面内容诱导的第三方 URL 外泄凭据);
    /// Cookie = 已有头 Cookie ← loginHeader Cookie ← cookie 库(请求注册域)按 key 去重合并;
    /// 全部值剥 CR/LF(已落盘的脏数据不致让 reqwest 构建失败、拖垮该书源全部请求)。
    pub(super) fn apply_auth(&self, url: &str, headers: &mut HashMap<String, String>) {
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

    /// 发请求(带登录态)→ `enabledCookieJar` 时回灌 `Set-Cookie` → `loginCheckJs` 校验登录态,
    /// 返回**完整响应**(body + 可选渲染 DOM)。失效返回 [`BookSourceError::LoginExpired`]。
    /// 引擎所有取页统一经此;只需 body 的调用方走 [`Engine::run_request`]。
    pub(super) async fn run_request_full(&self, req: FetchRequest) -> Result<FetchResponse> {
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
        Ok(resp)
    }

    /// [`Engine::run_request_full`] 的便捷封装:只回 body(绝大多数取页路径)。
    pub(super) async fn run_request(&self, req: FetchRequest) -> Result<String> {
        Ok(self.run_request_full(req).await?.body)
    }

    /// 取页(带登录态 + 回灌 + 登录校验)。
    pub(super) async fn fetch_checked(&self, url: impl Into<String>) -> Result<String> {
        self.run_request(self.get_req(url)).await
    }

    /// `loginCheckJs`(响应期登录态校验,D10 第一版):脚本以 `result`=响应求值;
    /// 返回空 / `false` / `0` 视为登录失效 → 抛 [`BookSourceError::LoginExpired`] 提示用户重登。
    /// 空脚本或未启用 `js` feature 时为 no-op。
    pub(super) fn check_login(&self, response: &str) -> Result<()> {
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
    /// 把 chapter 层与引擎的 book/source 层 overlay 成单个扁平 `Vars`(`interpolate` 只吃扁平表)。
    /// 优先级 `source < book < chapter`(高优先级后插覆盖)= get 时 章节→书籍→书源 取第一个非空。
    pub(super) fn flatten(&self, chapter: &Vars) -> Vars {
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
    pub(super) async fn run_prelude(&self, steps: &[PreStep], chapter: &mut Vars) -> Result<()> {
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
                    RenderArgs::default(),
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
    pub(super) async fn send_templated_full(
        &self,
        url: &UrlOrRule,
        method: Method,
        body: Option<&UrlOrRule>,
        headers: &HashMap<String, String>,
        vars: &Vars,
        // 渲染 + 点击翻页参数(`render-fetcher` / `search-click-pagination`);
        // prelude 等普通请求传 `RenderArgs::default()`(全关闭 = reqwest 单页)。
        args: RenderArgs<'_>,
    ) -> Result<FetchResponse> {
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
        self.run_request_full(FetchRequest {
            url,
            method,
            body,
            headers: hdrs,
            render: args.render,
            ready_for: args.ready_for.map(str::to_string),
            intercept_api: args.intercept_api.map(str::to_string),
            page: args.page,
            page_by: args.page_by.map(str::to_string),
        })
        .await
    }

    /// [`Engine::send_templated_full`] 的便捷封装:只回 body(prelude / 不需 DOM 的取页)。
    pub(super) async fn send_templated(
        &self,
        url: &UrlOrRule,
        method: Method,
        body: Option<&UrlOrRule>,
        headers: &HashMap<String, String>,
        vars: &Vars,
        args: RenderArgs<'_>,
    ) -> Result<String> {
        Ok(self
            .send_templated_full(url, method, body, headers, vars, args)
            .await?
            .body)
    }

    /// 对一段响应按 `capture` 顺序求值并写入各作用域层;空串不写(防污染低优先级层的非空值)。
    pub(super) fn capture_into(
        &self,
        caps: &[Capture],
        body: &str,
        chapter: &mut Vars,
    ) -> Result<()> {
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
    pub(super) async fn fetch_pages(
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

    pub(super) fn eval_list_items(
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

    /// 求值精确总页数(`render-dual-source`):按规则 `via` 路由源(见 [`pick_source`]),
    /// `via:css`/`xpath` 打渲染 DOM(分页器)、其余打 body。无规则或解析失败 → `None`
    /// (不阻断列表,仅少个进度数)。
    pub(super) fn eval_total_pages(
        &self,
        rule: Option<&Rule>,
        body: &str,
        dom: Option<&str>,
        vars: &Vars,
    ) -> Option<u32> {
        let rule = rule?;
        let s = eval_value(rule, pick_source(rule, body, dom), vars).ok()?;
        parse_total_pages(&s)
    }

    /// 求值「是否还有下一页」(`list-has-more`):求值结果**非空且非 `false`/`0`** → 还有下一页;
    /// 无规则 → `None`(不提供边界,UI 不限制);求值失败 → `None`(不误停)。源路由同 totalPages
    /// (按 `via`:番茄 `has_more` 是 `via:json` → 打 API body,即便同会话抓了 DOM 给 totalPages)。
    pub(super) fn eval_has_more(
        &self,
        rule: Option<&Rule>,
        body: &str,
        dom: Option<&str>,
        vars: &Vars,
    ) -> Option<bool> {
        let rule = rule?;
        match eval_value(rule, pick_source(rule, body, dom), vars) {
            Ok(s) => Some(!matches!(s.trim(), "" | "false" | "0")),
            Err(_) => None,
        }
    }

    pub(super) fn eval_book_info(&self, r: &BookRules, ctx: &str, vars: &Vars) -> Result<BookInfo> {
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

    pub(super) fn resolve_url(&self, u: &UrlOrRule, vars: &Vars) -> Result<String> {
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

    /// 共享列表页 runner(`dynamic-explore-entries`):search/explore 取一页书的统一实现——
    /// 取页 → 主请求 vars 捕获 → list/item 抽取 → totalPages/hasMore。`extra_vars` 是各操作注入的
    /// 额外变量(search 的 `{key}` / explore 的入口变量),与 base/page/pageSize 合并后驱动
    /// `spec.request`(render / interceptApi / pageBy 等均在其上)。`kind` 仅用于渲染结果缓存键前缀。
    pub(super) async fn run_list_page(
        &self,
        spec: &ListPageSpec,
        kind: char,
        extra_vars: &BTreeMap<String, String>,
        page: u32,
        page_size: u32,
    ) -> Result<BookList> {
        let req = &spec.request;
        // 渲染结果缓存(仅 render 路径):同 (操作,入口/词,页,页大小) 命中即返回,免重新驱动浏览器
        // 点击翻页(UI 回翻/重访的 O(N) 点击成本只在首访付一次)。
        let cache_key = req
            .render
            .then(|| list_cache_key(kind, extra_vars, page, page_size));
        if let Some(hit) = cache_key.as_deref().and_then(|k| self.cached_page(k)) {
            return Ok(hit);
        }
        let mut chapter = self.base_vars();
        chapter.extend(extra_vars.iter().map(|(k, v)| (k.clone(), v.clone())));
        chapter.insert("page".into(), page.to_string());
        chapter.insert("pageSize".into(), page_size.to_string());
        // 前置链(捕获 token 等)在主请求前跑一次。
        self.run_prelude(&spec.prelude, &mut chapter).await?;
        let vars = self.flatten(&chapter);
        // 完整响应:body(列表 / has_more 等)+ 可选渲染 DOM(via:css 的 totalPages,见 render-dual-source)。
        let resp = self
            .send_templated_full(
                &req.url,
                req.method,
                req.body.as_ref(),
                &req.headers,
                &vars,
                // 点击驱动翻页(search-click-pagination):URL 不认页码的 SPA 靠 pageBy.click 在一张
                // 活页点 page-1 次翻到目标页;page_by 缺席(如 URL 驱动的 explore)= `{{page}}` 进 URL 模板。
                RenderArgs {
                    render: req.render,
                    ready_for: req.ready_for.as_deref(),
                    intercept_api: req.intercept_api.as_deref(),
                    page,
                    page_by: req.page_by.as_ref().map(|p| p.click.as_str()),
                },
            )
            .await?;
        let html = &resp.body;
        // 主请求 vars 捕获(chapter 级):对响应求值,使 list/item 可见。各条**独立**对响应求值
        // (见 source `Request.vars` 契约「勿互相引用」,有序依赖应走 prelude 链)。
        let flat = self.flatten(&chapter);
        for (name, rule) in &req.vars {
            let v = eval_value(rule, html, &flat)?;
            if !v.is_empty() {
                chapter.insert(name.clone(), v);
            }
        }
        let vars = self.flatten(&chapter);
        let items = self.eval_list_items(&spec.list, &spec.item, html, &vars)?;
        let dom = resp.dom_html.as_deref();
        let total_pages = self.eval_total_pages(req.total_pages.as_ref(), html, dom, &vars);
        let has_more = self.eval_has_more(req.has_more.as_ref(), html, dom, &vars);
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

    /// 加载一个入口源 → 扁平 `ExploreEntry` 列表(静态固定入口直接映射;动态源走抓取)。
    pub(super) async fn load_entry_source(&self, src: &EntrySource) -> Result<Vec<ExploreEntry>> {
        match src {
            EntrySource::Static { static_entries } => Ok(static_entries
                .iter()
                .map(|e| ExploreEntry {
                    title: e.title.clone(),
                    vars: e.vars.clone(),
                })
                .collect()),
            EntrySource::Fetch { fetch } => self.load_fetch_entries(fetch).await,
        }
    }

    /// 加载远端抓取入口源:按 `forEach`(空 = 一次)循环,每次请求远端数据、`list` 抽项、每项经
    /// `item.title`/`item.vars` 求值生成入口。`item` 规则上下文 = 当前数据项;vars = base + 当前循环
    /// 变量(故规则既能 `via:json` 读项字段,也能 `{{name}}` 引用循环变量)。
    pub(super) async fn load_fetch_entries(
        &self,
        f: &FetchEntrySource,
    ) -> Result<Vec<ExploreEntry>> {
        // forEach 为空 = 执行一次(空循环变量);用切片避免 `Vec<&BTreeMap>` 收集与哨兵命名。
        let default = [BTreeMap::new()];
        let loops: &[BTreeMap<String, String>] = if f.for_each.is_empty() {
            &default
        } else {
            &f.for_each
        };
        let req = &f.request;
        let mut out = Vec::new();
        for loop_vars in loops {
            let mut chapter = self.base_vars();
            chapter.extend(loop_vars.iter().map(|(k, v)| (k.clone(), v.clone())));
            let vars = self.flatten(&chapter);
            let resp = self
                .send_templated_full(
                    &req.url,
                    req.method,
                    req.body.as_ref(),
                    &req.headers,
                    &vars,
                    RenderArgs {
                        render: req.render,
                        ready_for: req.ready_for.as_deref(),
                        intercept_api: req.intercept_api.as_deref(),
                        ..Default::default()
                    },
                )
                .await?;
            let html = &resp.body;
            for item_ctx in eval_list(&f.list, html)? {
                let title = eval_value(&f.item.title, &item_ctx, &vars)?;
                let mut evars = BTreeMap::new();
                for (name, rule) in &f.item.vars {
                    evars.insert(name.clone(), eval_value(rule, &item_ctx, &vars)?);
                }
                out.push(ExploreEntry { title, vars: evars });
            }
        }
        Ok(out)
    }
}

/// 渲染结果缓存键(`dynamic-explore-entries`):操作类型前缀 + 额外变量(`BTreeMap` 有序)+ 页 +
/// 页大小,各段以 `\0` 分隔。同入口/同词稳定命中;不同入口因变量段不同而不串缓存。仅 render 路径用。
fn list_cache_key(
    kind: char,
    extra: &BTreeMap<String, String>,
    page: u32,
    page_size: u32,
) -> String {
    let mut k = String::from(kind);
    for (name, val) in extra {
        k.push('\u{0}');
        k.push_str(name);
        k.push('=');
        k.push_str(val);
    }
    k.push('\u{0}');
    k.push_str(&page.to_string());
    k.push('\u{0}');
    k.push_str(&page_size.to_string());
    k
}

/// 求值一个可选规则;None 或空 → 空串。
pub(super) fn opt_eval(rule: Option<&Rule>, ctx: &str, vars: &Vars) -> Result<String> {
    Ok(match rule {
        Some(r) => eval_value(r, ctx, vars)?,
        None => String::new(),
    })
}

/// 双源路由(`render-dual-source`):`via:css`/`xpath` 的规则对渲染 DOM 求值(没抓到 DOM 则退 body),
/// 其余(json/regex/raw/纯值)对 body 求值。让 `has_more`(json→body)与 `total_pages`(css→DOM)
/// 在同一会话(抓了 DOM)下各打对的源。
fn pick_source<'a>(rule: &Rule, body: &'a str, dom: Option<&'a str>) -> &'a str {
    use crate::source::Via;
    match rule.primary_via() {
        Some(Via::Css | Via::Xpath) => dom.unwrap_or(body),
        _ => body,
    }
}

/// 从总页数规则的求值结果抽出 `u32`:取首段连续 ASCII 数字(容忍「99」「共99页」等;失败 → None)。
fn parse_total_pages(s: &str) -> Option<u32> {
    let s = s.trim();
    let start = s.find(|c: char| c.is_ascii_digit())?;
    s[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}
