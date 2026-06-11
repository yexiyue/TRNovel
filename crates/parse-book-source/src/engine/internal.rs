//! 引擎内部管线(私有 `impl Engine`):请求构造与登录态注入、取页与登录校验、模板化请求
//! 骨架、命名捕获、有界分页、列表/详情求值。均为 [`super::Engine`] 五个公开操作共享的实现细节。

use super::Engine;
use crate::error::{BookSourceError, Result};
use crate::eval::{Vars, eval_list, eval_value, interpolate};
use crate::fetch::cookie::{
    merge_login_into_headers, registrable_domain, request_registrable_domain,
};
use crate::fetch::{FetchRequest, FetchResponse};
use crate::model::{BookInfo, BookListItem};
use crate::source::{BookRules, Capture, Method, PreStep, Rule, UrlOrRule, VarScope};
use std::collections::HashMap;

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
    pub(super) async fn send_templated_full(
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
            render,
            ready_for: ready_for.map(str::to_string),
            intercept_api: intercept_api.map(str::to_string),
        })
        .await
    }

    /// [`Engine::send_templated_full`] 的便捷封装:只回 body(prelude / 不需 DOM 的取页)。
    #[allow(clippy::too_many_arguments)]
    pub(super) async fn send_templated(
        &self,
        url: &UrlOrRule,
        method: Method,
        body: Option<&UrlOrRule>,
        headers: &HashMap<String, String>,
        vars: &Vars,
        render: bool,
        ready_for: Option<&str>,
        intercept_api: Option<&str>,
    ) -> Result<String> {
        Ok(self
            .send_templated_full(
                url,
                method,
                body,
                headers,
                vars,
                render,
                ready_for,
                intercept_api,
            )
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
