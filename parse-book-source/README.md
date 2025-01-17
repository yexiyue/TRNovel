## Parse Book Source

本仓库是为TRNovle 服务，用于支持解析各种书籍源。兼容部分`阅读`书源。

- [x] 支持解析 Api Json接口
- [x] 支持解析 网站源

**示例**

```rust
use std::{thread::sleep, time::Duration};

use parse_book_source::{BookSource, BookSourceParser};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let book_source = BookSource::from_path(
        "./test.json",
    )?[0]
        .clone();
    let mut parser = BookSourceParser::new(book_source)?;
    // let res = parser.search_books("百炼", 1, 2).await?;
    // println!("{:#?}", res);
    let explores = parser.get_explores().await?;
    let res = parser.explore_books(&explores[0].url, 1, 2).await?;
    println!("{:#?}", res);
    let book_info = parser.get_book_info(&res[2].book_url).await?;
    println!("{:#?}", book_info);
    // sleep(Duration::from_secs(1));
    let toc = parser.get_chapters(&book_info.toc_url).await?;
    println!("{:#?}", toc);
    // sleep(Duration::from_secs(1));
    // let content = parser.get_content(&toc[1].chapter_url).await?;
    // println!("{}", toc[1].chapter_url);
    // println!("{}", content);
    Ok(())
}

```
