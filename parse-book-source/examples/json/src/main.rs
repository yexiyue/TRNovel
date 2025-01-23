use std::{path::PathBuf, str::FromStr, sync::Arc, thread::sleep, time::Duration};

use parse_book_source::{BookSource, BookSourceParser, Downloader};
use tokio::sync::Mutex;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let book_source = BookSource::from_path(
        "/Users/yexiyue/rust-project/ratatui-study/trnovel/parse-book-source/test.json",
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
    // let toc = parser.get_chapters(&book_info.toc_url).await?;
    // println!("{:#?}", toc);
    let downloader = Arc::new(Mutex::new(Downloader::new(&parser, book_info, 0)));

    tokio::spawn({
        let downloader = downloader.lock().await.clone();
        async move {
            sleep(Duration::from_secs(5));

            downloader.cancel();
            println!("cancel");
        }
    });

    downloader
        .lock()
        .await
        .download(
            PathBuf::from_str(
                "/Users/yexiyue/rust-project/ratatui-study/trnovel/parse-book-source/examples/json/aaa.txt",
            )
            .unwrap(),
            |chapter, a, b| {
                println!("{} {}/{}", chapter.chapter_name, a, b);
            },
        )
        .await?;
    // sleep(Duration::from_secs(1));
    // let content = parser.get_content(&toc[1].chapter_url).await?;
    // println!("{}", toc[1].chapter_url);
    // println!("{}", content);
    Ok(())
}
