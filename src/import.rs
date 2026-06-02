//! `import` 子命令:把书源 JSON(本地文件或 URL)导入 `~/.novel/book_sources.json`,
//! 使 AI 生成、经 `doctor` 验证过的书源**直接可用**于网络小说(无需进 TUI 手动导入)。
//!
//! 闭环:探站生成 → `trn doctor` 验证 → `trn import` 导入 → 在网络小说里选用。

use crate::book_source::BookSourceCache;
use parse_book_source::BookSource;

/// 导入指定路径或 URL 的书源(URL 走网络,文件直接解析)。
pub async fn run(source: &str) {
    let source = source.trim();
    let parsed = if source.starts_with("http://") || source.starts_with("https://") {
        BookSource::from_url(source).await
    } else {
        BookSource::from_path(source)
    };
    let sources = match parsed {
        Ok(s) if !s.is_empty() => s,
        Ok(_) => return eprintln!("✗ 未解析到任何书源(文件/URL 内容为空?)"),
        Err(e) => return eprintln!("✗ 解析书源失败: {e}"),
    };

    let mut cache = match BookSourceCache::load() {
        Ok(c) => c,
        Err(e) => return eprintln!("✗ 读取书源缓存失败: {e}"),
    };
    let names: Vec<String> = sources.iter().map(|b| b.name.clone()).collect();
    for bs in sources {
        // 按 url+name 去重:同名书源会被新的覆盖(便于反复迭代同一书源)。
        cache.add_book_source(bs);
    }
    if let Err(e) = cache.save() {
        return eprintln!("✗ 保存书源缓存失败: {e}");
    }

    let path = BookSourceCache::get_cache_file_path()
        .map(|p| p.display().to_string())
        .unwrap_or_else(|_| "~/.novel/book_sources.json".into());
    println!("✓ 已导入 {} 个书源 → {path}", names.len());
    for n in &names {
        println!("  - {n}");
    }
    println!("现在可在网络小说(`trn -n`)的书源列表里选用。");
}
