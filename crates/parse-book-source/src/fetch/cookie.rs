//! cookie 库:按**注册域(eTLD+1)**归并,session / persistent 分离。
//!
//! - `registrable_domain`:用 `psl` 公共后缀表取注册域(`example.co.uk` / `site.com.cn` 正确)。
//! - [`CookieJar`]:内存态 cookie 库,`Set-Cookie` 回灌(`enabledCookieJar`)、请求前合并进 `Cookie` 头;
//!   session cookie(无 `Expires`/`Max-Age`)仅内存、重启失效,persistent 可落盘([`CookieJar::persistent`])。
//!   `cf_clearance`、headful 登录 cookie、`Set-Cookie` 三路可汇入同一库。

use std::collections::{BTreeMap, HashMap};

/// 由 URL 或裸 host 取**注册域(eTLD+1)**作为 cookie 归并键。
/// `psl` 正确处理 `example.com` / `example.co.uk` / `site.com.cn`;IP / 单标签 / 未知后缀回退「末两段」。
/// 大小写归一(host 不区分大小写)。
pub fn registrable_domain(url: &str) -> String {
    let host = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .split(['/', '?', '#'])
        .next()
        .unwrap_or("")
        .split(':') // 去端口
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    // IPv4 优先判定:psl 会把纯数字 host 误套默认规则(192.168.1.1 → 1.1),故先短路。
    let labels: Vec<&str> = host.split('.').filter(|s| !s.is_empty()).collect();
    let is_ip = !labels.is_empty() && labels.iter().all(|l| l.chars().all(|c| c.is_ascii_digit()));
    if is_ip {
        return host;
    }
    if let Some(d) = psl::domain_str(&host) {
        return d.to_string();
    }
    if labels.len() >= 2 {
        labels[labels.len() - 2..].join(".")
    } else {
        host
    }
}

/// 解析一段 cookie 串(`k=v; k2=v2`)为有序 map:key/value 各自 trim、同名 last-wins、
/// 无 `=` 的段忽略。merge / 反序列化 / 按 key 查值统一走此函数,避免多处手写解析漂移。
pub(crate) fn parse_cookie_str(s: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for kv in s.split(';').map(str::trim).filter(|s| !s.is_empty()) {
        if let Some((k, v)) = kv.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

/// 合并两段 cookie 串(`k=v; k2=v2`)按 key 去重(`second` 同名覆盖 `first`),按字典序输出。
pub fn merge_cookie_str(first: &str, second: &str) -> String {
    let mut map = parse_cookie_str(first);
    map.extend(parse_cookie_str(second));
    pairs_to_str(&map)
}

/// `name -> value` map 序列化回 `k=v; k2=v2` 串(字典序)。
pub(crate) fn pairs_to_str(map: &BTreeMap<String, String>) -> String {
    map.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join("; ")
}

/// 剥除 header 值中的 CR/LF——防 header 注入,并避免「多个 Set-Cookie 以 `\n` 连接的值」
/// 被回写为出站请求头时让 reqwest 构建失败(审查确认的真问题)。
pub(crate) fn sanitize_header_value(v: &str) -> String {
    v.replace(['\r', '\n'], "")
}

/// 请求的注册域:绝对 URL 取其注册域,相对 URL 归一到书源注册域(`source_domain` 应已是注册域)。
pub(crate) fn request_registrable_domain(url: &str, source_domain: &str) -> String {
    if url.starts_with("http://") || url.starts_with("https://") {
        registrable_domain(url)
    } else {
        source_domain.to_string()
    }
}

/// 把登录态并入出站请求头——host(`net.*`)与引擎(`apply_auth`)共用的单一真相源:
///
/// - **同注册域门控**:仅当请求注册域与书源注册域一致时注入 loginHeader(含其 `Cookie` 字段),
///   防止「页面内容(toc/next_page/bookUrl 等)诱导的第三方绝对 URL」把 `Authorization`/Cookie
///   外泄(此防护针对可信书源 + 被挂马页面内容的威胁模型;恶意书源本可经脚本自行外泄,不在此列)。
///   相对 URL 已归一到书源域,天然放行;登录域与 API 域分属不同注册域的书源会被静默跳过,
///   如需支持留待书源 schema 显式声明授权域名集合。
/// - Cookie 合并优先级:已有头 Cookie ← loginHeader Cookie ← `jar_cookie`(调用方按请求注册域取,
///   本就按域隔离,不受门控),按 key 去重。
/// - 全部值剥 CR/LF(防 header 注入;亦兜底已落盘的脏数据)。
pub(crate) fn merge_login_into_headers(
    login_header: &BTreeMap<String, String>,
    source_domain: &str,
    request_domain: &str,
    jar_cookie: Option<&str>,
    headers: &mut HashMap<String, String>,
) {
    let mut cookie = headers
        .remove("Cookie")
        .or_else(|| headers.remove("cookie"));
    if request_domain == source_domain {
        for (k, v) in login_header {
            if k.eq_ignore_ascii_case("cookie") {
                let v = sanitize_header_value(v);
                cookie = Some(match cookie {
                    Some(c) => merge_cookie_str(&c, &v),
                    None => v,
                });
            } else {
                headers.insert(k.clone(), sanitize_header_value(v));
            }
        }
    }
    if let Some(jar) = jar_cookie {
        cookie = Some(match cookie {
            Some(c) => merge_cookie_str(&c, jar),
            None => jar.to_string(),
        });
    }
    // 最终再 sanitize 一次:cookie 值本身可能含 `\n`(如脏落盘数据),merge 不会剥除。
    if let Some(c) = cookie.map(|c| sanitize_header_value(&c))
        && !c.is_empty()
    {
        headers.insert("Cookie".into(), c);
    }
}

/// 一条 cookie 值 + 是否持久(有 `Expires`/`Max-Age`)。
#[derive(Debug, Clone, PartialEq, Eq)]
struct CookieVal {
    value: String,
    persistent: bool,
}

/// 内存态 cookie 库:`注册域 -> (name -> CookieVal)`。
#[derive(Debug, Clone, Default)]
pub struct CookieJar {
    jar: BTreeMap<String, BTreeMap<String, CookieVal>>,
}

impl CookieJar {
    /// 从持久化的 `注册域 -> "k=v; k2=v2"` 映射重建(全部标记为 persistent)。
    pub fn from_persistent(saved: &BTreeMap<String, String>) -> Self {
        let mut jar = BTreeMap::new();
        for (domain, cookie) in saved {
            let m: BTreeMap<String, CookieVal> = parse_cookie_str(cookie)
                .into_iter()
                .map(|(k, v)| {
                    (
                        k,
                        CookieVal {
                            value: v,
                            persistent: true,
                        },
                    )
                })
                .collect();
            if !m.is_empty() {
                jar.insert(registrable_domain(domain), m);
            }
        }
        Self { jar }
    }

    /// 取某域名(自动归一到注册域)的 `Cookie` 头串;空则 `None`。
    pub fn cookie_header(&self, domain: &str) -> Option<String> {
        let key = registrable_domain(domain);
        let m = self.jar.get(&key)?;
        if m.is_empty() {
            return None;
        }
        let flat: BTreeMap<String, String> = m
            .iter()
            .map(|(k, v)| (k.clone(), v.value.clone()))
            .collect();
        Some(pairs_to_str(&flat))
    }

    /// 回灌一条响应的 `Set-Cookie`(可能多条以 `\n` 连接)到某请求域名(归一到注册域)。
    /// `Max-Age<=0` 视为删除;有 `Expires`/`Max-Age(>0)` 为 persistent,否则 session。
    pub fn absorb_set_cookie(&mut self, request_domain: &str, set_cookie: &str) {
        let key = registrable_domain(request_domain);
        let entry = self.jar.entry(key).or_default();
        for line in set_cookie
            .split('\n')
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            let mut parts = line.split(';').map(str::trim);
            let Some(nv) = parts.next() else { continue };
            let Some((name, value)) = nv.split_once('=') else {
                continue;
            };
            let (name, value) = (name.trim().to_string(), value.trim().to_string());
            if name.is_empty() {
                continue;
            }
            let mut persistent = false;
            let mut deleted = false;
            for attr in parts {
                let lower = attr.to_ascii_lowercase();
                if let Some(ma) = lower.strip_prefix("max-age=") {
                    match ma.trim().parse::<i64>() {
                        Ok(n) if n <= 0 => deleted = true,
                        Ok(_) => persistent = true,
                        Err(_) => {}
                    }
                } else if lower.starts_with("expires=") {
                    persistent = true;
                }
            }
            if deleted {
                entry.remove(&name);
            } else {
                entry.insert(name, CookieVal { value, persistent });
            }
        }
        if entry.is_empty() {
            self.jar.remove(&registrable_domain(request_domain));
        }
    }

    /// 仅取 persistent cookie 的 `注册域 -> "k=v; k2=v2"` 映射,供落盘(session cookie 不保存)。
    pub fn persistent(&self) -> BTreeMap<String, String> {
        let mut out = BTreeMap::new();
        for (domain, m) in &self.jar {
            let flat: BTreeMap<String, String> = m
                .iter()
                .filter(|(_, v)| v.persistent)
                .map(|(k, v)| (k.clone(), v.value.clone()))
                .collect();
            if !flat.is_empty() {
                out.insert(domain.clone(), pairs_to_str(&flat));
            }
        }
        out
    }

    /// 库是否为空(无任何 cookie)。
    pub fn is_empty(&self) -> bool {
        self.jar.values().all(BTreeMap::is_empty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registrable_domain_publicsuffix_and_fallbacks() {
        assert_eq!(
            registrable_domain("https://www.fanqienovel.com/x"),
            "fanqienovel.com"
        );
        assert_eq!(registrable_domain("http://api.site.com:8080/p"), "site.com");
        assert_eq!(registrable_domain("WWW.Site.COM"), "site.com");
        assert_eq!(
            registrable_domain("https://www.example.com.cn/p"),
            "example.com.cn"
        );
        assert_eq!(
            registrable_domain("http://a.b.example.co.uk"),
            "example.co.uk"
        );
        assert_eq!(registrable_domain("http://192.168.1.1:80"), "192.168.1.1");
        assert_eq!(registrable_domain("localhost"), "localhost");
        assert_eq!(registrable_domain("http:///path"), "");
    }

    #[test]
    fn merge_cookie_str_dedups_second_wins() {
        assert_eq!(
            merge_cookie_str("sid=old; theme=dark", "sid=new; lang=zh"),
            "lang=zh; sid=new; theme=dark"
        );
    }

    #[test]
    fn sanitize_header_value_strips_crlf() {
        assert_eq!(
            sanitize_header_value("a=1; Path=/\nb=2; HttpOnly"),
            "a=1; Path=/b=2; HttpOnly"
        );
        assert_eq!(sanitize_header_value("Bearer\r\n token"), "Bearer token");
    }

    // ── 审查/security:loginHeader 仅注入同注册域请求;Cookie 三方合并;jar cookie 不受门控 ──
    #[test]
    fn merge_login_gates_cross_domain_and_merges_cookie() {
        let mut lh = BTreeMap::new();
        lh.insert("Authorization".into(), "Bearer T".into());
        lh.insert("Cookie".into(), "lang=zh".into());
        // 同注册域:loginHeader 注入,Cookie = 已有头 ← loginHeader ← jar 三方按 key 合并。
        let mut h = HashMap::new();
        h.insert("Cookie".into(), "a=1".into());
        merge_login_into_headers(&lh, "site.com", "site.com", Some("sid=9"), &mut h);
        assert_eq!(h.get("Authorization").map(String::as_str), Some("Bearer T"));
        assert_eq!(
            h.get("Cookie").map(String::as_str),
            Some("a=1; lang=zh; sid=9")
        );
        // 跨注册域:loginHeader(含其 Cookie)整体跳过,jar cookie(按请求域取)照常注入。
        let mut h2 = HashMap::new();
        merge_login_into_headers(&lh, "site.com", "evil.com", Some("sid=9"), &mut h2);
        assert!(!h2.contains_key("Authorization"), "跨域不应注入登录头");
        assert_eq!(h2.get("Cookie").map(String::as_str), Some("sid=9"));
    }

    #[test]
    fn absorb_splits_session_and_persistent() {
        let mut jar = CookieJar::default();
        // 子域请求 → 归并到注册域 site.com。
        jar.absorb_set_cookie(
            "www.site.com",
            "sid=abc; Path=/\nremember=1; Max-Age=3600; HttpOnly\ntmp=x; Path=/",
        );
        // 请求头含全部(session + persistent)。
        let header = jar.cookie_header("api.site.com").unwrap();
        assert!(header.contains("sid=abc"));
        assert!(header.contains("remember=1"));
        assert!(header.contains("tmp=x"));
        // 落盘只留 persistent(remember 有 Max-Age),session 的 sid/tmp 不保存。
        let persisted = jar.persistent();
        assert_eq!(
            persisted.get("site.com").map(String::as_str),
            Some("remember=1")
        );
    }

    #[test]
    fn absorb_max_age_zero_deletes() {
        let mut jar = CookieJar::default();
        jar.absorb_set_cookie("site.com", "sid=abc; Max-Age=3600");
        assert!(jar.cookie_header("site.com").unwrap().contains("sid=abc"));
        jar.absorb_set_cookie("site.com", "sid=; Max-Age=0");
        assert!(jar.cookie_header("site.com").is_none(), "Max-Age=0 应删除");
    }

    #[test]
    fn from_persistent_round_trip() {
        let mut saved = BTreeMap::new();
        saved.insert("site.com".to_string(), "a=1; b=2".to_string());
        let jar = CookieJar::from_persistent(&saved);
        assert_eq!(
            jar.cookie_header("www.site.com"),
            Some("a=1; b=2".to_string())
        );
        assert_eq!(
            jar.persistent().get("site.com").map(String::as_str),
            Some("a=1; b=2")
        );
    }

    #[test]
    fn expires_attribute_marks_persistent() {
        let mut jar = CookieJar::default();
        jar.absorb_set_cookie("site.com", "t=1; Expires=Wed, 09 Jun 2027 10:18:14 GMT");
        assert_eq!(
            jar.persistent().get("site.com").map(String::as_str),
            Some("t=1")
        );
    }
}
