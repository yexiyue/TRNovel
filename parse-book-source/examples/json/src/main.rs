use std::{thread::sleep, time::Duration};

use parse_book_source::{BookSource, BookSourceParser};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let book_source = BookSource::from_path(
        "/Users/yexiyue/rust-project/ratatui-study/trnovel/parse-book-source/test.json",
    )?[0]
        .clone();
    let mut parser = BookSourceParser::new(book_source)?;
    let res = parser.search_books("剑来", 1, 2).await?;
    // println!("{:#?}", res);
    // let explores = parser.explores().await?;
    // let res = parser.explore(&explores[0].url, 1, 2).await?;
    // println!("{:#?}", res);
    let book_info = parser.get_book_info(&res[0].book_url).await?;
    println!("{:#?}", book_info);
    sleep(Duration::from_secs(1));
    let toc = parser.get_chapters(&book_info.toc_url).await?;
    // println!("{:#?}", toc);
    sleep(Duration::from_secs(1));
    let content = parser.get_content(&toc[1].chapter_url).await?;
    println!("{}", toc[1].chapter_url);
    println!("{}", content);
    Ok(())
}
