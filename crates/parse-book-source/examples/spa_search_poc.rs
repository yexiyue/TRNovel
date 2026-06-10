//! 番茄网页搜索「渲染 + CDP 拦截」PoC(`render-fetcher` change 方式 B 验证)。
//!
//! 用法(先用 app/browser_login 登录过会员,profile 在 ~/.novel/browser-profile):
//! ```bash
//! cargo run -p parse-book-source --features browser --example spa_search_poc -- 十日终焉
//! ```
//! 用真浏览器打开 `/search/<词>`,让 SPA 自己 sec_sdk 签名 + 拉结果,CDP 拦截
//! `search_book/v1` 的响应体。验证:① 能拿到含 book_id 的 JSON;② 无头是否也行。

#[cfg(feature = "browser")]
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    use parse_book_source::{BrowserFetcher, BrowserOptions};
    use std::time::Duration;

    let keyword = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "十日终焉".to_string());
    let url = format!("https://fanqienovel.com/search/{keyword}");
    let api = "search_book/v1";

    let Some(browser) = BrowserFetcher::detect(BrowserOptions::default()) else {
        eprintln!("✗ 未探测到系统浏览器");
        std::process::exit(1);
    };

    for (label, headless) in [("headful(有头)", false), ("headless(无头)", true)] {
        eprintln!("\n========== {label} ==========");
        eprintln!("→ 打开 {url} ,CDP 拦截 {api} 响应(最多 30s)…");
        match browser
            .render_intercept(&url, api, Duration::from_secs(30), headless)
            .await
        {
            Ok(body) => {
                let _ = std::fs::write(
                    if headless {
                        "/tmp/search_headless.json"
                    } else {
                        "/tmp/search_headful.json"
                    },
                    &body,
                );
                println!("✅ 拦截到响应:{} 字节", body.len());
                // 解析关键字段,确认 book_id 在(DOM 里没有)。
                match serde_json::from_str::<serde_json::Value>(&body) {
                    Ok(v) => {
                        let s = v.to_string();
                        let count = s.matches("\"book_id\"").count();
                        println!("  book_id 出现 {count} 次");
                        for k in ["book_id", "book_name", "author", "abstract", "thumb_url"] {
                            if let Some(p) = s.find(&format!("\"{k}\"")) {
                                let frag: String = s[p..].chars().take(46).collect();
                                println!("  {frag}");
                            }
                        }
                    }
                    Err(e) => println!(
                        "  (非 JSON?{e};已存 /tmp,前 200 字符)\n  {}",
                        body.chars().take(200).collect::<String>()
                    ),
                }
            }
            Err(e) => println!("✗ {label} 失败: {e}"),
        }
    }
    eprintln!("\n结论:headful 拿到 JSON = 方式B可行;headless 也拿到 = 搜索可走无头(更快无窗口)。");
}

#[cfg(not(feature = "browser"))]
fn main() {
    eprintln!(
        "请加 --features browser:cargo run -p parse-book-source --features browser --example spa_search_poc -- 十日终焉"
    );
}
