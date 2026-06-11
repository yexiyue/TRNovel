//! 引擎离线单测:Fetcher 替身(Mock/Recording/CookieEcho/Scripted)驱动五个操作,
//! 覆盖分卷、登录态注入、跨注册域安全、换行净化、loginCheckJs、cookieJar 回灌、warmup、前置链捕获等。

use super::*;
use crate::error::FetchError;
use crate::fetch::{FetchRequest, FetchResponse, Fetcher};
use async_trait::async_trait;

use std::collections::HashMap;
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
