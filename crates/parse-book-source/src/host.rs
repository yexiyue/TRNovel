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

use crate::cookie::{
    CookieJar, merge_cookie_str, merge_login_into_headers, parse_cookie_str, registrable_domain,
    request_registrable_domain, sanitize_header_value,
};
use crate::error::EvalError;
use crate::eval::Vars;
use crate::fetch::{FetchRequest, FetchResponse, Fetcher};
use crate::js::{arg, register, to_eval, yield_js};
use crate::source::Method;
use crate::state::SourceState;
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
    register_host(&mut ctx).map_err(to_eval)?; // 追加 source/net
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

/// 注入 `source`(状态/登录)与 `net`(网络/cookie/浏览器)两个对象。
fn register_host(ctx: &mut Context) -> JsResult<()> {
    // source:书源 per-source 状态(跨请求 KV、单槽变量)+ 登录态(loginHeader 明文 / loginInfo 密文)。
    let source = ObjectInitializer::new(ctx)
        .function(NativeFunction::from_fn_ptr(js_put), js_string!("put"), 2)
        .function(NativeFunction::from_fn_ptr(js_get), js_string!("get"), 1)
        .function(
            NativeFunction::from_fn_ptr(js_get_variable),
            js_string!("getVariable"),
            0,
        )
        .function(
            NativeFunction::from_fn_ptr(js_put_variable),
            js_string!("putVariable"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(js_put_login_header),
            js_string!("putLoginHeader"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(js_get_login_header),
            js_string!("getLoginHeader"),
            0,
        )
        .function(
            NativeFunction::from_fn_ptr(js_get_login_header_map),
            js_string!("getLoginHeaderMap"),
            0,
        )
        .function(
            NativeFunction::from_fn_ptr(js_remove_login_header),
            js_string!("removeLoginHeader"),
            0,
        )
        .function(
            NativeFunction::from_fn_ptr(js_put_login_info),
            js_string!("putLoginInfo"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(js_get_login_info),
            js_string!("getLoginInfo"),
            0,
        )
        .function(
            NativeFunction::from_fn_ptr(js_get_login_info_map),
            js_string!("getLoginInfoMap"),
            0,
        )
        .build();
    ctx.register_global_property(js_string!("source"), source, Attribute::all())?;

    // net:网络出口与 cookie 读取(startBrowserAwait 由后续任务补全 7.x)。
    let net = ObjectInitializer::new(ctx)
        .function(NativeFunction::from_fn_ptr(js_ajax), js_string!("ajax"), 1)
        .function(
            NativeFunction::from_fn_ptr(js_connect),
            js_string!("connect"),
            1,
        )
        .function(NativeFunction::from_fn_ptr(js_post), js_string!("post"), 2)
        .function(
            NativeFunction::from_fn_ptr(js_get_cookie),
            js_string!("getCookie"),
            2,
        )
        .build();
    ctx.register_global_property(js_string!("net"), net, Attribute::all())?;
    Ok(())
}

// ───────────────────────── source.* native 函数 ─────────────────────────

/// `source.put(key, value)`:写入跨请求 KV,返回 value。
fn js_put(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let key = arg(args, 0, ctx)?;
    let value = arg(args, 1, ctx)?;
    if let Some(host) = active_host() {
        let mut h = host.borrow_mut();
        h.state.kv.insert(key, value.clone());
        h.dirty = true;
    }
    Ok(js_string!(value.as_str()).into())
}

/// `source.get(key)`:读取 KV,缺失返回空串(不抛错)。
fn js_get(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let key = arg(args, 0, ctx)?;
    let v = active_host()
        .map(|h| h.borrow().state.kv.get(&key).cloned().unwrap_or_default())
        .unwrap_or_default();
    Ok(js_string!(v.as_str()).into())
}

/// `source.getVariable()`:读取书源级单槽变量。
fn js_get_variable(_t: &JsValue, _args: &[JsValue], _ctx: &mut Context) -> JsResult<JsValue> {
    let v = active_host()
        .map(|h| h.borrow().state.variable.clone())
        .unwrap_or_default();
    Ok(js_string!(v.as_str()).into())
}

/// `source.putVariable(value)`:写入书源级单槽变量,返回 value。
fn js_put_variable(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let value = arg(args, 0, ctx)?;
    if let Some(host) = active_host() {
        let mut h = host.borrow_mut();
        h.state.variable = value.clone();
        h.dirty = true;
    }
    Ok(js_string!(value.as_str()).into())
}

// ── 登录态:loginHeader(明文 header map)与 loginInfo(加密凭据)──

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

/// 由字符串 map 构造一个 JS 对象 `{k: v, ...}`。
fn map_to_js_object(map: &BTreeMap<String, String>, ctx: &mut Context) -> JsObject {
    let mut init = ObjectInitializer::new(ctx);
    for (k, v) in map {
        init.property(
            js_string!(k.as_str()),
            js_string!(v.as_str()),
            Attribute::all(),
        );
    }
    init.build()
}

/// `source.putLoginHeader(json)`:用 JSON 对象**整体设置**登录态请求头(JWT/自定义头/Cookie 均可),
/// 含 `Cookie` 字段时同步并入 cookie 库;返回原 json;非法 JSON 抛 JS 异常。
fn js_put_login_header(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let json = arg(args, 0, ctx)?;
    let r = json_to_string_map(&json).map(|map| {
        if let Some(host) = active_host() {
            host.borrow_mut().set_login_header(map);
        }
        json.clone()
    });
    yield_js(r)
}

/// `source.getLoginHeader()`:返回登录态请求头的 JSON 字符串(为空返回空串)。
fn js_get_login_header(_t: &JsValue, _args: &[JsValue], _ctx: &mut Context) -> JsResult<JsValue> {
    let s = active_host()
        .map(|h| {
            let h = h.borrow();
            if h.state.login_header.is_empty() {
                String::new()
            } else {
                serde_json::to_string(&h.state.login_header).unwrap_or_default()
            }
        })
        .unwrap_or_default();
    Ok(js_string!(s.as_str()).into())
}

/// `source.getLoginHeaderMap()`:返回登录态请求头的 JS 对象。
fn js_get_login_header_map(
    _t: &JsValue,
    _args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let map = active_host()
        .map(|h| h.borrow().state.login_header.clone())
        .unwrap_or_default();
    Ok(map_to_js_object(&map, ctx).into())
}

/// `source.removeLoginHeader()`:清空登录态请求头。
fn js_remove_login_header(
    _t: &JsValue,
    _args: &[JsValue],
    _ctx: &mut Context,
) -> JsResult<JsValue> {
    if let Some(host) = active_host() {
        let mut h = host.borrow_mut();
        if !h.state.login_header.is_empty() {
            h.state.login_header.clear();
            h.dirty = true;
        }
    }
    Ok(JsValue::undefined())
}

/// `source.putLoginInfo(plain)`:加密存储登录凭据(机器绑定密钥),返回原文;失败抛 JS 异常。
fn js_put_login_info(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let plain = arg(args, 0, ctx)?;
    let r = match active_host() {
        Some(host) => host
            .borrow_mut()
            .store_login_info(&plain)
            .map(|_| plain.clone()),
        None => Ok(plain.clone()),
    };
    yield_js(r)
}

/// `source.getLoginInfo()`:解密返回登录凭据明文(未设置返回空串);失败抛 JS 异常。
fn js_get_login_info(_t: &JsValue, _args: &[JsValue], _ctx: &mut Context) -> JsResult<JsValue> {
    let r = match active_host() {
        Some(host) => host
            .borrow()
            .state
            .get_login_info()
            .map(Option::unwrap_or_default),
        None => Ok(String::new()),
    };
    yield_js(r)
}

/// `source.getLoginInfoMap()`:解密凭据并解析为 JS 对象(未设置返回空对象);失败抛 JS 异常。
fn js_get_login_info_map(_t: &JsValue, _args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let plain = match active_host() {
        Some(host) => match host.borrow().state.get_login_info() {
            Ok(o) => o.unwrap_or_default(),
            Err(e) => return Err(JsNativeError::typ().with_message(e.to_string()).into()),
        },
        None => String::new(),
    };
    if plain.is_empty() {
        return Ok(ObjectInitializer::new(ctx).build().into());
    }
    let map =
        json_to_string_map(&plain).map_err(|e| JsNativeError::typ().with_message(e.to_string()))?;
    Ok(map_to_js_object(&map, ctx).into())
}

// ───────────────────────── net.* native 函数 ─────────────────────────

/// 取第 `i` 个参数作为可选「额外请求头 JSON 串」:JS 对象会被 to_string 成
/// `"[object Object]"`,该值与空串都视为无额外头(headers 须经 JSON 串传入)。
fn opt_extra_arg(args: &[JsValue], i: usize, ctx: &mut Context) -> JsResult<Option<String>> {
    let extra = arg(args, i, ctx)?;
    Ok((!extra.is_empty() && extra != "[object Object]").then_some(extra))
}

/// 把请求结果转为 JS 值:成功构造 `{body, code, headers}` 对象,失败抛可被 `try/catch`
/// 捕获的 JS 异常(connect/post 共用收尾)。
fn yield_response(r: Result<FetchResponse, EvalError>, ctx: &mut Context) -> JsResult<JsValue> {
    match r {
        Ok(resp) => Ok(response_to_js(&resp, ctx).into()),
        Err(e) => Err(JsNativeError::typ().with_message(e.to_string()).into()),
    }
}

// 注:net.* 一律 `borrow_mut`(请求核心要把响应 Set-Cookie 写回 state.cookies);
// 网络调用是同线程同步 block_on,期间不会重入 JS,无 RefCell 双借风险。

/// `net.ajax(url)`:复用取页管线发 GET,返回响应体;失败抛 JS 异常(可 `try/catch`)。
fn js_ajax(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let url = arg(args, 0, ctx)?;
    let r = match active_host() {
        Some(host) => host.borrow_mut().ajax(&url),
        None => Err(EvalError::Host("no active host".into())),
    };
    yield_js(r)
}

/// `net.connect(url, extraHeadersJson?)`:发 GET 返回完整响应对象 `{body, code, headers}`
/// (供读 `Set-Cookie`/`Location`/状态码);失败抛 JS 异常。第二参为可选的额外请求头 JSON 串。
fn js_connect(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let url = arg(args, 0, ctx)?;
    let extra = opt_extra_arg(args, 1, ctx)?;
    let r = match active_host() {
        Some(host) => host.borrow_mut().connect(&url, extra.as_deref()),
        None => Err(EvalError::Host("no active host".into())),
    };
    yield_response(r, ctx)
}

/// `net.post(url, body, extraHeadersJson?)`:发 POST 返回完整响应对象 `{body, code, headers}`
/// (供表单/JSON 登录);失败抛 JS 异常。第三参为可选的额外请求头 JSON 串。
fn js_post(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let url = arg(args, 0, ctx)?;
    let body = arg(args, 1, ctx)?;
    let extra = opt_extra_arg(args, 2, ctx)?;
    let r = match active_host() {
        Some(host) => host.borrow_mut().post(&url, &body, extra.as_deref()),
        None => Err(EvalError::Host("no active host".into())),
    };
    yield_response(r, ctx)
}

/// 把 [`FetchResponse`] 构造成 JS 对象 `{body: string, code: number, headers: {k:v}}`。
fn response_to_js(resp: &FetchResponse, ctx: &mut Context) -> JsObject {
    let headers: BTreeMap<String, String> = resp
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let headers_obj = map_to_js_object(&headers, ctx);
    ObjectInitializer::new(ctx)
        .property(
            js_string!("body"),
            js_string!(resp.body.as_str()),
            Attribute::all(),
        )
        .property(
            js_string!("code"),
            JsValue::from(i32::from(resp.status)),
            Attribute::all(),
        )
        .property(js_string!("headers"), headers_obj, Attribute::all())
        .build()
}

/// `net.getCookie(domain, key?)`:读 cookie 库;给定 key 取该 cookie 值,否则返回整串。
fn js_get_cookie(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let domain = arg(args, 0, ctx)?;
    let key = arg(args, 1, ctx)?;
    let v = active_host()
        .map(|h| {
            h.borrow()
                .get_cookie(&domain, (!key.is_empty()).then_some(key.as_str()))
        })
        .unwrap_or_default();
    Ok(js_string!(v.as_str()).into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fetch::ReqwestFetcher;
    use crate::testutil::{book_source, spawn_echo_server, spawn_fixed_server};

    fn host_with(state: SourceState, base: &str) -> Rc<RefCell<SourceHost>> {
        let fetcher: Arc<dyn Fetcher> = Arc::new(ReqwestFetcher::new(&book_source(base)).unwrap());
        Rc::new(RefCell::new(SourceHost::new(base, state, fetcher).unwrap()))
    }

    // ── 3.5:source 与 net 是同一个 host 实例(一处写,另一处读得到)──
    #[test]
    fn source_and_net_share_same_host() {
        let mut state = SourceState::default();
        state.cookies.insert("site.com".into(), "sid=S".into());
        let host = host_with(state, "http://127.0.0.1:0");
        // source.put 写 KV、source.get 读回;net.getCookie 读同一 host 的 cookies。
        let out = eval_js_with_host(
            "source.put('a','1'); source.get('a') + ':' + net.getCookie('site.com','sid')",
            "",
            &Vars::new(),
            host,
        )
        .unwrap();
        assert_eq!(out, "1:S");
    }

    // ── 5.1:put/get 跨调用、缺失返回空串、getVariable/putVariable 单槽 ──
    #[test]
    fn put_get_and_variable_roundtrip() {
        let host = host_with(SourceState::default(), "http://127.0.0.1:0");
        let out = eval_js_with_host(
            "source.put('token','T'); \
             var miss = source.get('nope'); \
             source.putVariable('cfg'); \
             source.get('token') + '|' + miss + '|' + source.getVariable()",
            "",
            &Vars::new(),
            host.clone(),
        )
        .unwrap();
        assert_eq!(out, "T||cfg", "缺失键应为空串");
        // 状态确实写入 host(供调用方落盘),dirty 置位。
        let h = host.borrow();
        assert_eq!(h.state.kv.get("token").map(String::as_str), Some("T"));
        assert_eq!(h.state.variable, "cfg");
        assert!(h.dirty);
    }

    // ── 5.2:getCookie 按域名/键读取 ──
    #[test]
    fn get_cookie_by_domain_and_key() {
        let mut state = SourceState::default();
        state
            .cookies
            .insert("site.com".into(), "sid=abc; theme=dark".into());
        let host = host_with(state, "http://127.0.0.1:0");
        let out = eval_js_with_host(
            "net.getCookie('site.com','theme') + '|' + net.getCookie('site.com') + '|' + net.getCookie('other.com','x')",
            "",
            &Vars::new(),
            host,
        )
        .unwrap();
        assert_eq!(out, "dark|sid=abc; theme=dark|");
    }

    // ── 6.1:loginHeader put/get/getMap/remove 往返 ──
    #[test]
    fn login_header_put_get_remove_roundtrip() {
        let host = host_with(SourceState::default(), "http://127.0.0.1:0");
        let out = eval_js_with_host(
            "source.putLoginHeader(JSON.stringify({Authorization:'Bearer T', X:'1'})); \
             var s = source.getLoginHeader(); \
             var m = source.getLoginHeaderMap(); \
             var j = JSON.parse(s); \
             j.Authorization + '|' + m.X",
            "",
            &Vars::new(),
            host.clone(),
        )
        .unwrap();
        assert_eq!(out, "Bearer T|1");
        assert_eq!(
            host.borrow()
                .state
                .login_header
                .get("Authorization")
                .map(String::as_str),
            Some("Bearer T")
        );
        // remove 清空。
        let after = eval_js_with_host(
            "source.removeLoginHeader(); source.getLoginHeader()",
            "",
            &Vars::new(),
            host.clone(),
        )
        .unwrap();
        assert_eq!(after, "");
        assert!(host.borrow().state.login_header.is_empty());
    }

    // ── 8.3:loginInfo 加密存 / 解密读 / 解析为对象 ──
    #[test]
    fn login_info_encrypt_store_and_decrypt_read() {
        let host = host_with(SourceState::default(), "http://127.0.0.1:0");
        let out = eval_js_with_host(
            "source.putLoginInfo(JSON.stringify({user:'alice', pass:'pw密码'})); \
             var info = source.getLoginInfo(); \
             var m = source.getLoginInfoMap(); \
             JSON.parse(info).user + '|' + m.pass",
            "",
            &Vars::new(),
            host.clone(),
        )
        .unwrap();
        assert_eq!(out, "alice|pw密码");
        // 落盘字段是密文(非明文)。
        let ct = host.borrow().state.login_info.clone().unwrap();
        assert!(!ct.contains("alice"), "凭据应加密落盘: {ct}");
    }

    // ── 3.4:白名单——只有网络/状态/cookie 方法,无 fs/进程能力 ──
    #[test]
    fn host_exposes_only_whitelisted_capabilities() {
        let host = host_with(SourceState::default(), "http://127.0.0.1:0");
        let out = eval_js_with_host(
            "[typeof source.put, typeof source.get, typeof net.ajax, typeof net.getCookie, \
              typeof require, typeof process, typeof source.exec, typeof net.readFile].join(',')",
            "",
            &Vars::new(),
            host,
        )
        .unwrap();
        assert_eq!(
            out, "function,function,function,function,undefined,undefined,undefined,undefined",
            "只暴露白名单方法,无 require/process/exec/readFile"
        );
    }

    // ── 4.1 + 4.4 + 1.2:net.ajax 复用取页管线、自动带 loginHeader、spawn_blocking 不死锁 ──
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn net_ajax_reuses_pipeline_and_carries_login_header_without_deadlock() {
        let (base, server) = spawn_echo_server();
        let mut state = SourceState::default();
        state
            .login_header
            .insert("Authorization".into(), "Bearer testjwt".into());
        let fetcher: Arc<dyn Fetcher> = Arc::new(ReqwestFetcher::new(&book_source(&base)).unwrap());

        // 关键:在 spawn_blocking 线程上组装 host(独立 runtime + Rc 同线程)并求值。
        // 在主多线程 runtime 内通过独立 runtime block_on 网络——不死锁、不 panic。
        let url = base.clone();
        let (body, _state, _dirty) = tokio::task::spawn_blocking(move || {
            eval_blocking("net.ajax('/echo')", "", &Vars::new(), &url, state, fetcher)
        })
        .await
        .unwrap()
        .unwrap();

        server.join().unwrap();
        assert!(
            body.contains("GET /echo"),
            "应真实发出请求(回显含请求行): {body}"
        );
        // hyper 会把 header 名小写化,故大小写不敏感比对。
        assert!(
            body.to_ascii_lowercase()
                .contains("authorization: bearer testjwt"),
            "请求应自动携带 loginHeader: {body}"
        );
    }

    // ── 4.5:网络失败以可被 JS 捕获的方式返回(不使整段求值崩溃)──
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn net_ajax_failure_is_catchable() {
        // 连一个不监听的端口 → 连接失败;JS try/catch 应能捕获并返回兜底值。
        let fetcher: Arc<dyn Fetcher> =
            Arc::new(ReqwestFetcher::new(&book_source("http://127.0.0.1:1")).unwrap());
        let (out, _state, _dirty) = tokio::task::spawn_blocking(move || {
            eval_blocking(
                "try { net.ajax('/x'); 'NO_THROW' } catch(e) { 'CAUGHT' }",
                "",
                &Vars::new(),
                "http://127.0.0.1:1",
                SourceState::default(),
                fetcher,
            )
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(out, "CAUGHT", "网络失败应抛 JS 异常被 catch,而非 panic");
    }

    // ── 6.2:putLoginHeader 的 Cookie 字段同步进 cookie 库(按二级域名)──
    #[test]
    fn put_login_header_syncs_cookie_into_jar() {
        let host = host_with(SourceState::default(), "https://www.fanqienovel.com/reader");
        let out = eval_js_with_host(
            "source.putLoginHeader(JSON.stringify({Cookie:'sessionid=abc; uid=7'})); \
             net.getCookie('fanqienovel.com','sessionid') + '|' + net.getCookie('fanqienovel.com','uid')",
            "",
            &Vars::new(),
            host.clone(),
        )
        .unwrap();
        // 按二级域名 fanqienovel.com 归并(子域 www 共享)。
        assert_eq!(out, "abc|7");
        assert!(
            host.borrow().state.cookies.contains_key("fanqienovel.com"),
            "Cookie 应同步进 cookie 库"
        );
    }

    // ── 4.2:net.connect 返回 {body, code, headers}(可读响应头,如 Set-Cookie)──
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn net_connect_returns_body_code_and_headers() {
        // 起一个返回固定 Set-Cookie 头的服务(Content-Length 按字节算:"正文OK" 为 8 字节)。
        let body = "正文OK";
        let (base, server) = spawn_fixed_server(format!(
            "HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\nSet-Cookie: sid=xyz; Path=/\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ));

        let fetcher: Arc<dyn Fetcher> = Arc::new(ReqwestFetcher::new(&book_source(&base)).unwrap());
        let url = base.clone();
        let (out, _state, _dirty) = tokio::task::spawn_blocking(move || {
            eval_blocking(
                "var r = net.connect('/x'); \
                 r.code + '|' + r.body + '|' + r.headers['set-cookie']",
                "",
                &Vars::new(),
                &url,
                SourceState::default(),
                fetcher,
            )
        })
        .await
        .unwrap()
        .unwrap();
        server.join().unwrap();
        assert_eq!(out, "200|正文OK|sid=xyz; Path=/");
    }

    // ── 8.2:脚本登录壳——书源 login() 用 net.ajax 拿 token 后 putLoginHeader,登录态写回 state ──
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn run_login_script_writes_back_login_header() {
        // 服务返回固定 token 文本。
        let body = "TOKEN123";
        let (base, server) = spawn_fixed_server(format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        ));

        let login_js = "function login(){ \
             var tok = net.ajax('/api/token'); \
             source.putLoginHeader(JSON.stringify({Authorization:'Bearer '+tok})); \
           }";
        let fetcher: Arc<dyn Fetcher> = Arc::new(ReqwestFetcher::new(&book_source(&base)).unwrap());
        let url = base.clone();
        let (state, dirty) = tokio::task::spawn_blocking(move || {
            run_login(login_js, &url, SourceState::default(), fetcher)
        })
        .await
        .unwrap()
        .unwrap();
        server.join().unwrap();
        assert!(dirty, "登录写回应置 dirty");
        assert_eq!(
            state.login_header.get("Authorization").map(String::as_str),
            Some("Bearer TOKEN123"),
            "login() 应通过 net.ajax 取 token 并写回 loginHeader"
        );
    }

    // ── 审查/correctness:merge_cookie_jar 与已存在 cookie 合并 + 同名覆盖 ──
    #[test]
    fn merge_cookie_jar_overwrites_same_key_keeps_others() {
        let mut jar = BTreeMap::new();
        jar.insert("x.com".to_string(), "sid=old; theme=dark".to_string());
        merge_cookie_jar(&mut jar, "x.com", "sid=new; lang=zh");
        // sid 覆盖为 new、theme 保留、lang 新增,按 BTreeMap 字典序输出。
        assert_eq!(
            jar.get("x.com").map(String::as_str),
            Some("lang=zh; sid=new; theme=dark")
        );
        // 空 add 不破坏既有(仅规整化重排)。
        merge_cookie_jar(&mut jar, "x.com", "   ");
        assert_eq!(
            jar.get("x.com").map(String::as_str),
            Some("lang=zh; sid=new; theme=dark")
        );
    }

    // registrable_domain / sanitize_header_value 边界用例见 crate::cookie 模块单测(此处不重复)。

    // ── 审查/correctness:getCookie 用页面子域读取应回退到注册域命中(写读对齐)──
    #[test]
    fn get_cookie_falls_back_to_registrable_domain() {
        let mut state = SourceState::default();
        state
            .cookies
            .insert("fanqienovel.com".into(), "sid=abc; theme=dark".into());
        let host = host_with(state, "https://www.fanqienovel.com");
        // 脚本用页面实际 host(www. 子域,且大写)读取,应回退注册域命中。
        let out = eval_js_with_host(
            "net.getCookie('www.fanqienovel.com','sid') + '|' + net.getCookie('API.Fanqienovel.com','theme')",
            "",
            &Vars::new(),
            host,
        )
        .unwrap();
        assert_eq!(out, "abc|dark");
    }

    // ── 审查/test-coverage:多个 Set-Cookie 经 net.connect 以 \n 连接呈现(登录脚本依赖)──
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn net_connect_joins_multiple_set_cookie_with_newline() {
        let (base, server) = spawn_fixed_server(
            "HTTP/1.1 200 OK\r\nSet-Cookie: a=1; Path=/\r\nSet-Cookie: b=2; HttpOnly\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                .to_string(),
        );
        let fetcher: Arc<dyn Fetcher> = Arc::new(ReqwestFetcher::new(&book_source(&base)).unwrap());
        let url = base.clone();
        let (out, _s, _d) = tokio::task::spawn_blocking(move || {
            eval_blocking(
                "var r = net.connect('/x'); r.headers['set-cookie'].split('\\n').length + '|' + r.headers['set-cookie']",
                "",
                &Vars::new(),
                &url,
                SourceState::default(),
                fetcher,
            )
        })
        .await
        .unwrap()
        .unwrap();
        server.join().unwrap();
        // 两个 Set-Cookie 以 \n 连接为单串,split('\n') 得 2 条。
        assert_eq!(out, "2|a=1; Path=/\nb=2; HttpOnly");
    }

    // ── 审查/test-coverage:connect 额外头 —— JSON 串正路径叠加在 loginHeader 之上 ──
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn net_connect_extra_headers_stack_over_login_header() {
        let (base, server) = spawn_echo_server();
        let mut state = SourceState::default();
        state
            .login_header
            .insert("Authorization".into(), "Bearer T".into());
        let fetcher: Arc<dyn Fetcher> = Arc::new(ReqwestFetcher::new(&book_source(&base)).unwrap());
        let url = base.clone();
        let (body, _s, _d) = tokio::task::spawn_blocking(move || {
            eval_blocking(
                "net.connect('/x', JSON.stringify({'X-Test':'42'})).body",
                "",
                &Vars::new(),
                &url,
                state,
                fetcher,
            )
        })
        .await
        .unwrap()
        .unwrap();
        server.join().unwrap();
        let lower = body.to_ascii_lowercase();
        assert!(lower.contains("x-test: 42"), "额外头应送出: {body}");
        assert!(
            lower.contains("authorization: bearer t"),
            "loginHeader 应保留: {body}"
        );
    }

    // ── 审查/correctness:connect 额外头传非法 JSON 串 → 抛错可被 catch(不再静默吞)──
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn net_connect_bad_extra_headers_throws() {
        // 非法 JSON 在发请求前即失败,无需服务。
        let fetcher: Arc<dyn Fetcher> =
            Arc::new(ReqwestFetcher::new(&book_source("http://127.0.0.1:1")).unwrap());
        let (out, _s, _d) = tokio::task::spawn_blocking(move || {
            eval_blocking(
                "try { net.connect('/x', 'not-json'); 'NO_THROW' } catch(e) { 'CAUGHT' }",
                "",
                &Vars::new(),
                "http://127.0.0.1:1",
                SourceState::default(),
                fetcher,
            )
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(out, "CAUGHT", "非法额外头 JSON 应抛错被捕获,而非静默丢弃");
    }

    // ── 审查/correctness:含 \n 的 Cookie loginHeader 经 sanitize 后不致后续请求构建失败 ──
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn newline_in_login_header_does_not_break_request() {
        let (base, server) = spawn_echo_server();
        let mut state = SourceState::default();
        // 模拟脚本把 \n 连接的多 Set-Cookie 直接写回 Cookie(naive 用法)。
        state
            .login_header
            .insert("Cookie".into(), "a=1\nb=2".into());
        let fetcher: Arc<dyn Fetcher> = Arc::new(ReqwestFetcher::new(&book_source(&base)).unwrap());
        let url = base.clone();
        let (body, _s, _d) = tokio::task::spawn_blocking(move || {
            eval_blocking("net.ajax('/x')", "", &Vars::new(), &url, state, fetcher)
        })
        .await
        .unwrap()
        .unwrap();
        server.join().unwrap();
        // 请求成功送出(未因 \n 触发 reqwest builder error),且 Cookie 行无裸换行。
        assert!(body.contains("GET /x"), "请求应成功送出: {body}");
        let lower = body.to_ascii_lowercase();
        assert!(
            lower.contains("cookie: a=1b=2"),
            "Cookie 的 \\n 应被剥除: {body}"
        );
    }

    // ── 10.4:出站请求带上持久化 cookie(按注册域),并与 loginHeader 的 Cookie 合并去重 ──
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn net_request_sends_persisted_cookie_merged_with_login_cookie() {
        let (base, server) = spawn_echo_server();
        let domain = registrable_domain(&base); // 127.0.0.1(IP 回退)
        let mut state = SourceState::default();
        // 上次会话持久化的 cookie(按注册域存)。
        state.cookies.insert(domain, "sid=persisted".into());
        // 另有直接设在 loginHeader 的 Cookie,应与库 cookie 合并。
        state.login_header.insert("Cookie".into(), "lang=zh".into());
        let fetcher: Arc<dyn Fetcher> = Arc::new(ReqwestFetcher::new(&book_source(&base)).unwrap());
        let url = base.clone();
        let (body, _s, _d) = tokio::task::spawn_blocking(move || {
            eval_blocking("net.ajax('/x')", "", &Vars::new(), &url, state, fetcher)
        })
        .await
        .unwrap()
        .unwrap();
        server.join().unwrap();
        let lower = body.to_ascii_lowercase();
        // 两条 cookie 合并送出(字典序:lang 在前)。
        assert!(
            lower.contains("cookie: lang=zh; sid=persisted"),
            "应合并发送持久化与登录 cookie: {body}"
        );
    }

    // ── 审查/correctness:net.* 响应的 Set-Cookie 回灌 state.cookies(cookie 会话型脚本登录)──
    // 此前只进 reqwest 内部 cookie_store(fetcher 销毁即丢),落盘登录态为空 → 下次必判失效。
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn net_response_set_cookie_absorbed_into_state() {
        let (base, server) = spawn_fixed_server(
            "HTTP/1.1 200 OK\r\nSet-Cookie: session=S1; Path=/; HttpOnly\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                .to_string(),
        );
        let fetcher: Arc<dyn Fetcher> = Arc::new(ReqwestFetcher::new(&book_source(&base)).unwrap());
        let url = base.clone();
        let (out, state, dirty) = tokio::task::spawn_blocking(move || {
            eval_blocking(
                // 登录脚本 POST 后,session cookie 应立即可经 net.getCookie 读到。
                "net.connect('/login'); net.getCookie('127.0.0.1','session')",
                "",
                &Vars::new(),
                &url,
                SourceState::default(),
                fetcher,
            )
        })
        .await
        .unwrap()
        .unwrap();
        server.join().unwrap();
        assert_eq!(out, "S1", "Set-Cookie 应回灌进 state.cookies 供脚本读取");
        assert!(dirty, "回灌应置 dirty(供调用方落盘)");
        assert!(
            state
                .cookies
                .get("127.0.0.1")
                .is_some_and(|c| c.contains("session=S1")),
            "登录态应随 state 返回供落盘: {:?}",
            state.cookies
        );
    }

    // ── 审查/test-coverage:net.post 发 POST + body(spec source-auth 要求,表单登录用)──
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn net_post_sends_body() {
        let (base, server) = spawn_echo_server();
        let fetcher: Arc<dyn Fetcher> = Arc::new(ReqwestFetcher::new(&book_source(&base)).unwrap());
        let url = base.clone();
        let (body, _s, _d) = tokio::task::spawn_blocking(move || {
            eval_blocking(
                "net.post('/login', 'user=alice&pass=x').body",
                "",
                &Vars::new(),
                &url,
                SourceState::default(),
                fetcher,
            )
        })
        .await
        .unwrap()
        .unwrap();
        server.join().unwrap();
        assert!(body.contains("POST /login"), "应为 POST: {body}");
        assert!(body.contains("user=alice&pass=x"), "body 应送出: {body}");
    }

    // ── 审查/test-coverage:run_login 失败传播 + 未定义 login 的空操作语义 ──
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn run_login_propagates_throw_and_noops_when_undefined() {
        // (1) login() 显式 throw → run_login 返回 Err 且含错误信息。
        let f1: Arc<dyn Fetcher> =
            Arc::new(ReqwestFetcher::new(&book_source("http://127.0.0.1:1")).unwrap());
        let r1 = tokio::task::spawn_blocking(move || {
            run_login(
                "function login(){ throw new Error('bad cred'); }",
                "http://127.0.0.1:1",
                SourceState::default(),
                f1,
            )
        })
        .await
        .unwrap();
        let err = r1.unwrap_err().to_string();
        assert!(err.contains("bad cred"), "登录脚本抛错应传播: {err}");

        // (2) 脚本未定义 login 函数 → 固定壳跳过 → Ok 且 dirty=false、登录态为空(调用方据此判断未登录)。
        let f2: Arc<dyn Fetcher> =
            Arc::new(ReqwestFetcher::new(&book_source("http://127.0.0.1:1")).unwrap());
        let (state, dirty) = tokio::task::spawn_blocking(move || {
            run_login(
                "var x = 1;",
                "http://127.0.0.1:1",
                SourceState::default(),
                f2,
            )
        })
        .await
        .unwrap()
        .unwrap();
        assert!(!dirty, "未执行 login 不应置 dirty");
        assert!(state.login_header.is_empty(), "未登录则登录态为空");
    }

    // ── 审查/test-coverage:getLoginInfoMap 无凭据返回空对象、非 JSON 凭据抛错 ──
    #[test]
    fn get_login_info_map_empty_and_non_json() {
        // 无凭据 → 空对象。
        let host = host_with(SourceState::default(), "http://127.0.0.1:0");
        let out = eval_js_with_host(
            "Object.keys(source.getLoginInfoMap()).length.toString()",
            "",
            &Vars::new(),
            host,
        )
        .unwrap();
        assert_eq!(out, "0", "未设置凭据时返回空对象");

        // 裸 token(非 JSON 对象)→ getLoginInfo 正常、getLoginInfoMap 抛错被捕获。
        let host = host_with(SourceState::default(), "http://127.0.0.1:0");
        let out = eval_js_with_host(
            "source.putLoginInfo('not-json-token'); \
             var raw = source.getLoginInfo(); \
             var m; try { source.getLoginInfoMap(); m='NO_THROW'; } catch(e){ m='CAUGHT'; } \
             raw + '|' + m",
            "",
            &Vars::new(),
            host,
        )
        .unwrap();
        assert_eq!(out, "not-json-token|CAUGHT");
    }
}
