//! JS host 桥单测:source/net 两对象的 KV/变量/cookie/loginHeader/loginInfo 往返、
//! 网络请求与 Set-Cookie 回灌、spawn_blocking 无死锁、登录脚本等。

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

// registrable_domain / sanitize_header_value 边界用例见 crate::fetch::cookie 模块单测(此处不重复)。

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
