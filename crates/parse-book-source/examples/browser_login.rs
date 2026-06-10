//! headful 浏览器登录(7.x)手动验收示例。
//!
//! 用法:
//! ```bash
//! cargo run -p parse-book-source --features browser --example browser_login -- <login_url> [cookie_name...]
//! ```
//! 打开系统浏览器到 `<login_url>`,你在真实页面手动登录。
//! - 传了 `cookie_name` → 任一目标 cookie 出现即自动判定成功;
//! - 也可随时回到本终端按 Enter 表示「登录完成」。
//!
//! 成功后打印:登录后 URL、按注册域归并的 cookie(含 HttpOnly)、localStorage 键、页面 HTML 长度。

#[cfg(feature = "browser")]
#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    use parse_book_source::{BrowserFetcher, BrowserOptions, LoginCriteria, LoginSignal};
    use std::sync::atomic::Ordering;

    let mut args = std::env::args().skip(1);
    let Some(url) = args.next() else {
        eprintln!(
            "用法: cargo run -p parse-book-source --features browser --example browser_login -- <login_url> [cookie_name...]"
        );
        std::process::exit(2);
    };
    let cookie_names: Vec<String> = args.collect();

    let Some(browser) = BrowserFetcher::detect(BrowserOptions::default()) else {
        eprintln!("✗ 未探测到系统 Chromium 系浏览器(Chrome/Edge/Brave/…),无法验收登录");
        std::process::exit(1);
    };

    let criteria = LoginCriteria {
        cookie_names: cookie_names.clone(),
        local_storage_keys: Vec::new(),
    };
    let signal = LoginSignal::default();

    // 终端按 Enter → 置 done(用户「登录完成」)。
    {
        let done = signal.done.clone();
        std::thread::spawn(move || {
            let mut s = String::new();
            let _ = std::io::stdin().read_line(&mut s);
            done.store(true, Ordering::Relaxed);
        });
    }

    if cookie_names.is_empty() {
        eprintln!("→ 在弹出的浏览器里登录,完成后回到本终端按 Enter…");
    } else {
        eprintln!("→ 在弹出的浏览器里登录;cookie {cookie_names:?} 任一出现即自动完成(或按 Enter)…");
    }

    match browser.login(&url, &criteria, &signal).await {
        Ok(out) => {
            println!("\n✓ 登录成功,最终 URL: {}", out.url);
            println!("cookie(按注册域,含 HttpOnly):");
            for (d, c) in out.cookies_by_registrable_domain() {
                println!("  {d}: {c}");
            }
            println!(
                "localStorage 键: {:?}",
                out.local_storage.keys().collect::<Vec<_>>()
            );
            println!("登录后页面 HTML 长度: {} 字节", out.html.len());
        }
        Err(e) => {
            eprintln!("✗ 登录失败/取消: {e}");
            std::process::exit(1);
        }
    }
}

#[cfg(not(feature = "browser"))]
fn main() {
    eprintln!(
        "请加 --features browser 运行此示例:cargo run -p parse-book-source --features browser --example browser_login -- <login_url>"
    );
}
