//! 渲染型搜索的「完整引擎路径」验收:Engine(带浏览器)→ search → 渲染+CDP拦截 → via:json
//! → fontMap 解码 → BookListItem。验证 `render-fetcher` 端到端(需登录过的 browser-profile)。
//!
//! ```bash
//! cargo run -p parse-book-source --features browser --example engine_search_poc -- fanqie-web.v2.json 十日终焉
//! ```

#[cfg(feature = "browser")]
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    use parse_book_source::{BookSource, BrowserFetcher, BrowserOptions, Engine};

    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "fanqie-web.v2.json".to_string());
    let key = std::env::args()
        .nth(2)
        .unwrap_or_else(|| "末日生存方案".to_string());

    let json = std::fs::read_to_string(&path).expect("读取书源失败");
    let source = BookSource::from_json(&json).expect("解析书源失败");
    let browser = BrowserFetcher::detect(BrowserOptions::default());
    if browser.is_none() {
        eprintln!("✗ 未探测到浏览器,无法渲染搜索");
        std::process::exit(1);
    }
    let engine = Engine::with_browser_assist(source, browser).expect("构建引擎失败");

    eprintln!("→ 渲染+CDP拦截搜索「{key}」(headless,复用登录 profile)…");
    match engine.search(&key, 1, 10).await {
        Ok(books) => {
            println!(
                "\n✅ 搜索返回 {} 本(名/作者经搜索 fontMap 解码,book_id→bookUrl):",
                books.len()
            );
            for b in &books {
                println!("  {} / {} → {}", b.info.name, b.info.author, b.book_url);
            }
        }
        Err(e) => eprintln!("✗ 搜索失败: {e}"),
    }
}

#[cfg(not(feature = "browser"))]
fn main() {
    eprintln!("需 --features browser");
}
