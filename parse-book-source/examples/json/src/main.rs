use std::{fs::File, thread::sleep, time::Duration};

use parse_book_source::{utils::Params, BookSource, JsonSource};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let file = File::open("../../test.json")?;
    let book_source: BookSource = serde_json::from_reader(file)?;
    let mut json_source = JsonSource::try_from(book_source)?;

    let book_list = json_source
        .search_books(Params::new().key("剑来").page(1).page_size(2))
        .await?;
    // let book_list = json_source
    //     .explore_books(
    //         &json_source.explores.as_ref().unwrap()[0],
    //         Params::new().page(1).page_size(5),
    //     )
    //     .await?;

    println!("{:#?}", book_list);
    println!("等待中");
    sleep(Duration::from_secs(1));
    println!("等待结束");

    let book_info = json_source.book_info(&book_list[0]).await?;
    println!("{:#?}", book_info);

    println!("等待中");
    sleep(Duration::from_secs(1));
    println!("等待结束");

    let chapter_list = json_source.chapter_list(&book_info).await?;
    println!("{:#?}", chapter_list);

    println!("等待中");
    sleep(Duration::from_secs(1));
    println!("等待结束");
    let chapter = json_source.chapter_content(&chapter_list[0]).await?;

    println!("{}", chapter);
    Ok(())
}
