//! 书源登录动作(9.x / 12.1):脚本登录、浏览器登录。
//!
//! 产物(loginHeader / cookies / 加密 loginInfo)写回 per-source 状态并落盘
//! ([`crate::cache::save_source_state`]),由 [`crate::browser_assist::build_engine`] 注入每个请求。
//! 登录态过期(TTL)在加载时清理,失效请求由引擎抛 `LoginExpired` 提示重登。

use crate::cache::{load_source_state, save_source_state};
use anyhow::anyhow;
use parse_book_source::cookie::{merge_cookie_str, registrable_domain};
use parse_book_source::{
    BookSource, BrowserFetcher, BrowserOptions, Fetcher, LoginCriteria, LoginSignal, ReqwestFetcher,
};
use std::collections::BTreeMap;
use std::sync::Arc;

// 「是否需要登录」的判定即 `BookSource::has_login`(loginUrl/loginUi 任一非空),
// 不再做 app 侧同名包装,避免两个 crate 各持一套口径漂移。

/// 该书源应走**脚本登录**(`loginUrl` 是 `@js:`/`<js>` 脚本)还是**浏览器登录**(普通 URL)。
pub fn is_script_login(source: &BookSource) -> bool {
    source.get_login_js().is_some()
}

/// 该书源是否**已登录**:per-source 状态里存有有效 cookie 或 loginHeader。
/// `load_source_state` 会在加载时清理 TTL 过期登录态,故过期即视为未登录。
pub fn is_logged_in(source_url: &str) -> bool {
    let state = load_source_state(source_url);
    !state.cookies.is_empty() || !state.login_header.is_empty()
}

/// 把 loginUi 收集到的字段值拼成 loginInfo 的明文 JSON 对象串(供 `login()` 脚本 `getLoginInfo` 读取)。
pub fn login_info_json(fields: &[(String, String)]) -> String {
    let map: BTreeMap<&str, &str> = fields
        .iter()
        .map(|(k, v)| (k.as_str(), v.as_str()))
        .collect();
    serde_json::to_string(&map).unwrap_or_else(|_| "{}".into())
}

/// 脚本登录:加密保存凭据(若提供)→ 在 `spawn_blocking` 跑 `login()`(host `net.*` 需 `block_on`,
/// 必经非 async 线程)→ 产物(loginHeader/cookie)随状态落盘。
pub async fn script_login(mut source: BookSource, login_info: Option<String>) -> crate::Result<()> {
    let login_js = source
        .get_login_js()
        .ok_or_else(|| anyhow!("该书源 loginUrl 不是登录脚本(@js:/<js>),应走浏览器登录"))?
        .to_string();
    // 登录脚本的每次网络调用必须有界:reqwest 默认**无超时**,书源未配 http.timeout 时
    // 注入 30s 默认——否则挂起不响应的登录端点会让 spawn_blocking 里的 block_on 无限阻塞
    //(登录页只能 Esc 放弃等待,挂死线程会泄漏到进程退出,故必须由超时兜底)。
    if source.http.timeout.is_none() {
        source.http.timeout = Some(30_000);
    }
    let url = source.url.clone();
    let mut state = load_source_state(&url);
    if let Some(plain) = login_info {
        state
            .set_login_info(&plain)
            .map_err(|e| anyhow!("凭据加密失败: {e}"))?;
    }
    let fetcher: Arc<dyn Fetcher> =
        Arc::new(ReqwestFetcher::new(&source).map_err(|e| anyhow!("构建取页器失败: {e}"))?);
    let url2 = url.clone();
    let (new_state, _dirty) = tokio::task::spawn_blocking(move || {
        parse_book_source::host::run_login(&login_js, &url2, state, fetcher)
    })
    .await
    .map_err(|e| anyhow!("登录任务异常: {e}"))?
    .map_err(|e| anyhow!("登录脚本执行失败: {e}"))?;
    save_source_state(&url, &new_state)?;
    Ok(())
}

/// 浏览器登录:headful 打开 `loginUrl` 让用户手动登录,提取 cookie(含 HttpOnly)+ localStorage 里的
/// JWT 落盘。`signal` 由 UI 持同一 `Arc`:用户「完成」置 `done`、「取消」置 `cancel`。
pub async fn browser_login(source: BookSource, signal: LoginSignal) -> crate::Result<()> {
    let target = source.login_url.trim();
    if target.is_empty() {
        return Err(anyhow!("该书源未配置 loginUrl,无法浏览器登录").into());
    }
    let browser = BrowserFetcher::detect(BrowserOptions::default())
        .ok_or_else(|| anyhow!("未探测到系统浏览器(Chrome/Edge/Brave/…),无法浏览器登录"))?;
    // 复位上次尝试残留的 done/cancel 标志(信号跨重试共享同一 Arc):
    // 残留 cancel 会让重试首轮即判「用户取消」;残留 done 会把未登录空产物当成功落盘。
    signal.reset();
    // 第一版靠用户在 TUI 确认完成;后续可按站点配置目标 cookie/localStorage 键自动判定。
    let criteria = LoginCriteria::default();
    let outcome = browser
        .login(target, &criteria, &signal)
        .await
        .map_err(|e| anyhow!("浏览器登录失败/取消: {e}"))?;

    let url = source.url.clone();
    let mut state = load_source_state(&url);
    // 浏览器 cookie(含 HttpOnly)按注册域并入登录态(子域共享)。
    // 域过滤:登录页常嵌第三方 SSO/验证码 iframe,共享 browser-profile 里还残留其它站点会话,
    // 只保留与登录流相关的注册域(loginUrl / 书源自身 / 登录后最终 URL,后者覆盖重定向式 SSO),
    // 避免无关第三方 cookie 进入该书源状态、被书源脚本经 net.getCookie 读取外泄。
    let allowed = [
        registrable_domain(target),
        registrable_domain(&url),
        registrable_domain(&outcome.url),
    ];
    for (dom, c) in outcome.cookies_by_registrable_domain() {
        if !allowed.contains(&dom) {
            continue;
        }
        // merge 而非整域覆盖:与 host/engine 对同一 state.cookies 的 merge 语义一致,
        // 浏览器新值同名优先;浏览器本次未带的旧键残留靠 TTL 清理兜底。
        let merged = merge_cookie_str(
            state.cookies.get(&dom).map(String::as_str).unwrap_or(""),
            &c,
        );
        state.cookies.insert(dom, merged);
    }
    // localStorage 里的 JWT(常见键名)→ loginHeader 的 `Authorization: Bearer`。
    if let Some(tok) = ["access_token", "token", "authorization", "jwt"]
        .iter()
        .find_map(|k| outcome.local_storage.get(*k))
        .filter(|t| !t.is_empty())
    {
        let bearer = if tok.starts_with("Bearer ") {
            tok.clone()
        } else {
            format!("Bearer {tok}")
        };
        state.login_header.insert("Authorization".into(), bearer);
    }
    save_source_state(&url, &state)?;
    Ok(())
}
