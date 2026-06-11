//! JS host 桥(`js-host` feature):给 boa 沙箱注入两个职责分明的有状态对象——
//! `source`(书源状态/登录:`put`/`get`/`getVariable`/`putVariable`/…)与
//! `net`(网络/cookie/浏览器:`ajax`/`getCookie`/…);加解密沿用现有 `crypto`。
//! 二者底层共享同一 [`SourceHost`] 实例。是登录与多步请求编排的运行时基础设施
//! (见 change `js-host-bridge` 的 design D2/D3)。
//!
//! ## 安全白名单
//! host 只暴露**网络 / per-source 状态 / cookie / 浏览器**四类能力,
//! **绝不暴露文件系统、进程或任意主机命令**(对齐 Legado 的 `enableDangerousApi` 约束)。
//! 故意不沿用 Legado 的 `java` 命名(无 Java 语义、误导)。
//!
//! ## 线程模型(关键)
//! `Rc<RefCell<SourceHost>>` 与 boa `Context` 都是**线程亲和(`!Send`)**的,必须在
//! **同一线程内组装、求值、析构**。host 自带一个**独立 current-thread runtime**,网络调用
//! 在其上 `block_on`——因此整个求值必须跑在**非 runtime worker 的线程**上(异步引擎用
//! [`tokio::task::spawn_blocking`] 调度 [`eval_blocking`]):spawn_blocking 线程不处于
//! async 上下文,`block_on` 既不借用主 worker、也不触发 nested-runtime panic,杜绝死锁
//! (design D2 / 决策 1.2)。
//!
//! host 持**专属 fetcher**(自己的 reqwest Client),只在独立 runtime 上使用,避免与引擎
//! 主 runtime 共享同一连接池带来的跨 runtime 隐患。

/// `source`/`net` 对象注册与 native 函数实现(供 [`eval_js_with_host`] 调用)。
mod register;
/// JS host 持久状态(加密凭据 / KV / 登录态),`js-host` 专属。
pub mod state;

use crate::error::EvalError;
use crate::eval::Vars;
use crate::eval::js::{arg, register, to_eval, yield_js};
use crate::fetch::cookie::{
    CookieJar, merge_cookie_str, merge_login_into_headers, parse_cookie_str, registrable_domain,
    request_registrable_domain, sanitize_header_value,
};
use crate::fetch::{FetchRequest, FetchResponse, Fetcher};
use crate::host::state::SourceState;
use crate::source::Method;
use boa_engine::object::ObjectInitializer;
use boa_engine::property::Attribute;
use boa_engine::{
    Context, JsNativeError, JsObject, JsResult, JsValue, NativeFunction, Source, js_string,
};
use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};
use std::rc::Rc;
use std::sync::Arc;

/// 一次 JS 求值期间为书源 JS 提供的有状态宿主:per-source 状态 + 网络出口。
///
/// 线程亲和:持有独立 runtime,只能在创建它的那个线程上使用与析构(见模块文档)。
pub struct SourceHost {
    /// per-source 持久状态(kv / variable / login_header / login_info / cookies)。
    pub state: SourceState,
    /// 本次求值是否修改了 `state`——调用方据此决定是否落盘(避免无谓写盘)。
    pub dirty: bool,
    /// 书源二级域名(注册域),作为 cookie 库的归并键(完整 publicsuffix 归并见 10.x)。
    domain: String,
    /// 取页器:`net.*` 复用完整取页管线(resolve / 解码 / 重试 / 限速)。
    /// 应为 host **专属**实例,只在 `rt` 上使用。
    fetcher: Arc<dyn Fetcher>,
    /// 独立 current-thread runtime:在 spawn_blocking 线程上 `block_on` 网络调用。
    rt: tokio::runtime::Runtime,
}

impl SourceHost {
    /// 用书源 URL、per-source 状态与专属 fetcher 组装 host,自建独立 runtime。
    ///
    /// 必须在打算运行求值的那个线程上调用(runtime 与后续的 `Rc`/`Context` 同线程)。
    pub fn new(
        source_url: &str,
        state: SourceState,
        fetcher: Arc<dyn Fetcher>,
    ) -> Result<Self, EvalError> {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| EvalError::Host(format!("create host runtime: {e}")))?;
        Ok(Self {
            state,
            dirty: false,
            domain: registrable_domain(source_url),
            fetcher,
            rt,
        })
    }

    /// 组装出站请求头(与引擎 `apply_auth` 共用 [`merge_login_into_headers`],单一真相源):
    /// loginHeader 仅注入**同注册域**请求(JWT/自定义头/Cookie 同路径,防第三方 URL 外泄凭据)、
    /// 按 URL 注册域取**持久化 cookie**、全部值剥 CR/LF;可选额外头在合并之后**整体覆盖**
    /// (含 `Cookie`:直接替换而非合并,脚本显式传入即以脚本为准)。
    fn outbound_headers(
        &self,
        url: &str,
        extra: Option<BTreeMap<String, String>>,
    ) -> HashMap<String, String> {
        let mut headers = HashMap::new();
        // 请求 URL 的注册域(相对 URL → 书源自身注册域)。
        let domain = request_registrable_domain(url, &self.domain);
        let jar_cookie = self.state.cookies.get(&domain);
        merge_login_into_headers(
            &self.state.login_header,
            &self.domain,
            &domain,
            jar_cookie.map(String::as_str),
            &mut headers,
        );
        if let Some(extra) = extra {
            for (k, v) in extra {
                headers.insert(k, sanitize_header_value(&v));
            }
        }
        headers
    }

    /// `net.*` 的共享请求核心(ajax/connect/post 复用):组请求 → 独立 runtime 上 `block_on`
    /// 完整取页 → 响应 `Set-Cookie` 回灌 `state.cookies`(见 [`SourceHost::absorb_set_cookie`])。
    /// `label` 保持 JS 可见错误前缀不变(`"ajax {url}: …"` 等);`extra_headers` 为可选 JSON 对象串。
    /// 假定 `fetch_full` 与 `fetch` 取页语义一致(现有全部实现满足),`ajax` 经此取 body。
    fn request(
        &mut self,
        label: &str,
        method: Method,
        url: &str,
        body: Option<String>,
        extra_headers: Option<&str>,
    ) -> Result<FetchResponse, EvalError> {
        let extra = match extra_headers {
            Some(j) => Some(json_to_string_map(j)?),
            None => None,
        };
        let req = FetchRequest {
            url: url.to_string(),
            method,
            body,
            headers: self.outbound_headers(url, extra),
            ..Default::default()
        };
        let fetcher = self.fetcher.clone();
        let resp = self
            .rt
            .block_on(fetcher.fetch_full(req))
            .map_err(|e| EvalError::Host(format!("{label} {url}: {e}")))?;
        self.absorb_set_cookie(url, &resp);
        Ok(resp)
    }

    /// 把响应的 `Set-Cookie`(多条以 `\n` 连接)按请求注册域并入 `state.cookies` 并置 `dirty`——
    /// 否则 cookie 会话型脚本登录(表单登录最常见形态)的登录态只活在 fetcher 内部,落盘为空。
    ///
    /// 注:`state.cookies` 是平面 map,**刻意不分 session/persistent**(与引擎 `CookieJar` 的
    /// 分离语义有意分叉):登录脚本场景以「登录产物持久化」为意图,行为与 browser_login 一致,
    /// 请勿"统一"回 CookieJar 语义。也不受 `enabledCookieJar` 开关门控(登录路径默认回灌)。
    fn absorb_set_cookie(&mut self, url: &str, resp: &FetchResponse) {
        let Some(set_cookie) = resp.headers.get("set-cookie") else {
            return;
        };
        let domain = request_registrable_domain(url, &self.domain);
        // 复用 CookieJar 的 Set-Cookie 解析(含 `Max-Age<=0` 删除、`\n` 多条拆分),再展平回平面 map。
        let mut saved = BTreeMap::new();
        if let Some(existing) = self.state.cookies.get(&domain) {
            saved.insert(domain.clone(), existing.clone());
        }
        let mut jar = CookieJar::from_persistent(&saved);
        jar.absorb_set_cookie(&domain, set_cookie);
        match jar.cookie_header(&domain) {
            Some(c) => {
                self.state.cookies.insert(domain, c);
            }
            None => {
                self.state.cookies.remove(&domain);
            }
        }
        self.dirty = true;
    }

    /// `net.ajax`:复用取页管线发一个 GET,自动附带当前书源的 loginHeader(JWT/Cookie 同路径)。
    /// 在独立 runtime 上 `block_on`,失败转 [`EvalError::Host`](可被 JS `try/catch` 捕获)。
    fn ajax(&mut self, url: &str) -> Result<String, EvalError> {
        self.request("ajax", Method::Get, url, None, None)
            .map(|r| r.body)
    }

    /// `net.connect`:复用取页管线发 GET,返回**完整响应**(body + 状态码 + 响应头),
    /// 供登录脚本读 `Set-Cookie` / `Location` / 状态码。`extra_headers`(可选 JSON 对象串)叠加在
    /// loginHeader 之上;非法 JSON 抛错(不再静默吞)。在独立 runtime 上 `block_on`。
    fn connect(
        &mut self,
        url: &str,
        extra_headers: Option<&str>,
    ) -> Result<FetchResponse, EvalError> {
        self.request("connect", Method::Get, url, None, extra_headers)
    }

    /// `net.post`:发 POST(带 body)返回完整响应,供脚本做表单/JSON 登录(spec source-auth 要求)。
    /// `extra_headers`(可选 JSON 对象串)叠加在 loginHeader 之上;非法 JSON 抛错。
    fn post(
        &mut self,
        url: &str,
        body: &str,
        extra_headers: Option<&str>,
    ) -> Result<FetchResponse, EvalError> {
        self.request(
            "post",
            Method::Post,
            url,
            Some(body.to_string()),
            extra_headers,
        )
    }

    /// 整体设置登录态请求头(写入侧即剥 CR/LF,保证落盘数据干净——脚本常把 `\n` 连接的多
    /// `Set-Cookie` 直接写回);若含 `Cookie`/`cookie` 字段,同步并入 cookie 库(按二级域名)。置 `dirty`。
    fn set_login_header(&mut self, mut map: BTreeMap<String, String>) {
        for v in map.values_mut() {
            *v = sanitize_header_value(v);
        }
        self.state.login_header = map;
        if let Some(cookie) = self
            .state
            .login_header
            .get("Cookie")
            .or_else(|| self.state.login_header.get("cookie"))
            .cloned()
        {
            merge_cookie_jar(&mut self.state.cookies, &self.domain, &cookie);
        }
        self.dirty = true;
    }

    /// 加密写入登录凭据(机器绑定密钥)并置 `dirty`(供调用方落盘)。
    fn store_login_info(&mut self, plain: &str) -> Result<(), EvalError> {
        self.state.set_login_info(plain)?;
        self.dirty = true;
        Ok(())
    }

    /// `net.getCookie`:读 cookie 库中某域名的 cookie;给定 `key` 时取该 cookie 的值,否则返回整串。
    ///
    /// 域名查找:先按调用方传入域名(小写归一)精确查,未命中再回退其二级域名——
    /// 与写入端(按二级域名归并)对齐,使脚本用页面实际 host(如 `www.`/`api.` 子域)也能读回。
    fn get_cookie(&self, domain: &str, key: Option<&str>) -> String {
        let domain = domain.to_ascii_lowercase();
        let jar = self.state.cookies.get(&domain).or_else(|| {
            let reg = registrable_domain(&domain);
            (reg != domain)
                .then(|| self.state.cookies.get(&reg))
                .flatten()
        });
        let Some(jar) = jar else {
            return String::new();
        };
        match key {
            None | Some("") => jar.clone(),
            // 统一走 parse_cookie_str(trim、同名 last-wins),与 merge/序列化的归一化对齐。
            Some(k) => parse_cookie_str(jar).get(k).cloned().unwrap_or_default(),
        }
    }
}

// ───────────────────────── 线程局部 host(单线程同步求值)─────────────────────────
//
// boa native function 是无捕获的 `fn` 指针,无法直接携带 `Rc<RefCell<SourceHost>>`
// (boa 的 GC 捕获需 `Trace`,而 SourceHost 持 runtime/Client 不应实现 Trace)。
// 求值全程在单线程上同步进行,故用 thread-local 暂存 host,native fn 从中取——
// 安全且与现有 `crypto` 的 `from_fn_ptr` 风格一致。RAII guard 保证 host 仅在本次
// 求值期间可见,结束即清。

thread_local! {
    static ACTIVE_HOST: RefCell<Option<Rc<RefCell<SourceHost>>>> = const { RefCell::new(None) };
}

/// 安装 host 到当前线程,作用域结束(drop)即清除。
struct HostGuard;

impl HostGuard {
    fn install(host: Rc<RefCell<SourceHost>>) -> Self {
        ACTIVE_HOST.with(|slot| *slot.borrow_mut() = Some(host));
        HostGuard
    }
}

impl Drop for HostGuard {
    fn drop(&mut self) {
        ACTIVE_HOST.with(|slot| *slot.borrow_mut() = None);
    }
}

/// 取当前线程安装的 host(克隆出 `Rc`,不跨 native fn 持有 thread-local 借用)。
fn active_host() -> Option<Rc<RefCell<SourceHost>>> {
    ACTIVE_HOST.with(|slot| slot.borrow().clone())
}

/// 把新 cookie 串按 key 合并进某域名的现有 cookie(同名覆盖),回写 jar。
/// `add` 先剥 CR/LF(写入侧净化,保证落盘数据干净)。
fn merge_cookie_jar(jar: &mut BTreeMap<String, String>, domain: &str, add: &str) {
    let add = sanitize_header_value(add);
    let merged = merge_cookie_str(jar.get(domain).map(String::as_str).unwrap_or(""), &add);
    jar.insert(domain.to_string(), merged);
}

/// 把 JSON 对象字符串解析为字符串 header map(非字符串值用其 JSON 表示)。
fn json_to_string_map(json: &str) -> Result<BTreeMap<String, String>, EvalError> {
    let v: serde_json::Value = serde_json::from_str(json)
        .map_err(|e| EvalError::Host(format!("invalid json object: {e}")))?;
    let obj = v
        .as_object()
        .ok_or_else(|| EvalError::Host("expected json object".into()))?;
    Ok(obj
        .iter()
        .map(|(k, val)| {
            let s = match val {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            (k.clone(), s)
        })
        .collect())
}

// ───────────────────────── 求值入口 ─────────────────────────

/// 以给定 host 求值一段 JS:注入 `result`/`baseUrl`/变量/`crypto` + `source`/`net`。
///
/// 调用方须保证当前线程**不处于 async 上下文**(若脚本用 `net.*`):见模块文档的线程模型。
/// 一般经 [`eval_blocking`] 在 [`tokio::task::spawn_blocking`] 中调用。
pub fn eval_js_with_host(
    script: &str,
    result: &str,
    vars: &Vars,
    host: Rc<RefCell<SourceHost>>,
) -> Result<String, EvalError> {
    let _guard = HostGuard::install(host);
    let mut ctx = Context::default();
    register(&mut ctx, result, vars).map_err(to_eval)?; // result/baseUrl/vars/crypto(复用纯沙箱)
    register::register_host(&mut ctx).map_err(to_eval)?; // 追加 source/net
    let value = ctx.eval(Source::from_bytes(script)).map_err(to_eval)?;
    Ok(value
        .to_string(&mut ctx)
        .map_err(to_eval)?
        .to_std_string_escaped())
}

/// Send 友好的求值入口:在**当前线程**组装 host(独立 runtime + `Rc`,均线程亲和)、求值、
/// 拆出结果与可能被修改的 `state`。供异步引擎用 `spawn_blocking(move || eval_blocking(...))`
/// 调度——传入的 `state`/`fetcher` 都是 `Send`,`Rc`/`Context` 绝不跨线程。
///
/// 返回 `(JS 求值结果, 求值后 state, state 是否被修改)`;`dirty` 为 `true` 时调用方应落盘。
///
/// # Panics
/// 若在 **tokio runtime worker 线程(async 执行上下文)** 内直接调用必 panic:脚本用 `net.*`
/// 时是 `block_on` 的「Cannot start a runtime from within a runtime」;即便**纯状态脚本不触网**,
/// host 自带的 `Runtime` 在 async 上下文中 **drop 同样 panic**(tokio 禁止在异步上下文析构
/// runtime)。故无论脚本是否触网,**必须**经 [`tokio::task::spawn_blocking`] 调度
/// (spawn_blocking 线程非 async 上下文)。
pub fn eval_blocking(
    script: &str,
    result: &str,
    vars: &Vars,
    source_url: &str,
    state: SourceState,
    fetcher: Arc<dyn Fetcher>,
) -> Result<(String, SourceState, bool), EvalError> {
    let host = Rc::new(RefCell::new(SourceHost::new(source_url, state, fetcher)?));
    let out = eval_js_with_host(script, result, vars, host.clone())?;
    // 求值结束:guard 已 drop(thread-local 清空),host 为唯一持有者,可拆出 state。
    let host = Rc::try_unwrap(host)
        .map_err(|_| EvalError::Host("host still referenced after eval".into()))?
        .into_inner();
    Ok((out, host.state, host.dirty))
}

/// 跑书源登录脚本:在 `login_js` 后拼固定壳调用其 `login()` 入口(`login.apply(this)`),
/// host 注入 `source`/`net`,脚本内用 `source.getLoginInfo()` 取凭据、`net.connect/ajax` 发请求、
/// `source.putLoginHeader(...)` 写回登录态。返回 `(求值后 state, dirty)`。
///
/// 用户触发登录时调用(经 `spawn_blocking`);登录产物(loginHeader/cookie)随 `state` 落盘复用。
///
/// # Panics
/// 同 [`eval_blocking`]:登录脚本必经 `net.*`,故**必须**经 [`tokio::task::spawn_blocking`]
/// 调度,否则在 runtime worker 线程内会因 `block_on` panic。
pub fn run_login(
    login_js: &str,
    source_url: &str,
    state: SourceState,
    fetcher: Arc<dyn Fetcher>,
) -> Result<(SourceState, bool), EvalError> {
    let wrapped = format!("{login_js}\n;if(typeof login=='function'){{login.apply(this);}}\n");
    let (_, state, dirty) = eval_blocking(&wrapped, "", &Vars::new(), source_url, state, fetcher)?;
    Ok((state, dirty))
}

#[cfg(test)]
mod tests;
