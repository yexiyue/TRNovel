## Parse Book Source

本仓库是为TRNovler 服务，用于支持解析各种书籍源。

- [x] 支持解析 Api Json接口
- [ ] 支持解析 网站源

**示例**
```rust
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

```


**书籍源示例**

```json
{
  "bookSourceGroup": "API",
  "bookSourceName": "熊猫看书",
  "bookSourceType": 0,
  "bookSourceUrl": "https://anduril.xmkanshu.com",
  "customOrder": 5,
  "enabled": true,
  "enabledCookieJar": true,
  "enabledExplore": true,
  "exploreUrl": "[{\"title\":\"男生频道\",\"url\":\"/category/getbooklist?columntype=99&pageid={{page}}&pagesize=20&book_size=-1&category_id1=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":1,\"layout_flexGrow\":1}},{\"title\":\"都市娱乐\",\"url\":\"/category/getbooklist?columntype=99&pageid={{page}}&pagesize=20&book_size=-1&category_id1=1001&category_id2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"奇幻玄幻\",\"url\":\"/category/getbooklist?columntype=99&pageid={{page}}&pagesize=20&book_size=-1&category_id1=1002&category_id2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"武侠仙侠\",\"url\":\"/category/getbooklist?columntype=99&pageid={{page}}&pagesize=20&book_size=-1&category_id1=1003&category_id2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"历史军事\",\"url\":\"/category/getbooklist?columntype=99&pageid={{page}}&pagesize=20&book_size=-1&category_id1=1004&category_id2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"科幻末世\",\"url\":\"/category/getbooklist?columntype=99&pageid={{page}}&pagesize=20&book_size=-1&category_id1=1005&category_id2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"悬疑灵异\",\"url\":\"/category/getbooklist?columntype=99&pageid={{page}}&pagesize=20&book_size=-1&category_id1=1006&category_id2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"同人小说\",\"url\":\"/category/getbooklist?columntype=99&pageid={{page}}&pagesize=20&book_size=-1&category_id1=1007&category_id2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"二次元\",\"url\":\"/category/getbooklist?columntype=99&pageid={{page}}&pagesize=20&book_size=-1&category_id1=1008&category_id2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"经管小说\",\"url\":\"/category/getbooklist?columntype=99&pageid={{page}}&pagesize=20&book_size=-1&category_id1=1014&category_id2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"女生频道\",\"url\":\"/category/getbooklist?columntype=98&pageid={{page}}&pagesize=20&book_size=-1&category_id1=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":1,\"layout_flexGrow\":1}},{\"title\":\"现代言情\",\"url\":\"/category/getbooklist?columntype=98&pageid={{page}}&pagesize=20&book_size=-1&category_id1=1009&category_id2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"古代言情\",\"url\":\"/category/getbooklist?columntype=98&pageid={{page}}&pagesize=20&book_size=-1&category_id1=1010&category_id2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"幻想时空\",\"url\":\"/category/getbooklist?columntype=98&pageid={{page}}&pagesize=20&book_size=-1&category_id1=1011&category_id2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"次元专区\",\"url\":\"/category/getbooklist?columntype=98&pageid={{page}}&pagesize=20&book_size=-1&category_id1=1012&category_id2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"同人小说\",\"url\":\"/category/getbooklist?columntype=98&pageid={{page}}&pagesize=20&book_size=-1&category_id1=1013&category_id2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"\",\"url\":\"\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"出版频道\",\"url\":\"/category/getbooklist?columntype=6000&pageid={{page}}&pagesize=20&book_size=-1&typeid=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":1,\"layout_flexGrow\":1}},{\"title\":\"文学全部\",\"url\":\"/category/getbooklist?columntype=6000&pageid={{page}}&pagesize=20&book_size=-1&typeid=6001&typeid2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"小说全部\",\"url\":\"/category/getbooklist?columntype=6000&pageid={{page}}&pagesize=20&book_size=-1&typeid=6002&typeid2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"青春文学\",\"url\":\"/category/getbooklist?columntype=6000&pageid={{page}}&pagesize=20&book_size=-1&typeid=6003&typeid2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"传记全部\",\"url\":\"/category/getbooklist?columntype=6000&pageid={{page}}&pagesize=20&book_size=-1&typeid=6004&typeid2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"经济管理\",\"url\":\"/category/getbooklist?columntype=6000&pageid={{page}}&pagesize=20&book_size=-1&typeid=6005&typeid2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"成功励志\",\"url\":\"/category/getbooklist?columntype=6000&pageid={{page}}&pagesize=20&book_size=-1&typeid=6006&typeid2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"人物社科\",\"url\":\"/category/getbooklist?columntype=6000&pageid={{page}}&pagesize=20&book_size=-1&typeid=6007&typeid2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"历史文化\",\"url\":\"/category/getbooklist?columntype=6000&pageid={{page}}&pagesize=20&book_size=-1&typeid=6008&typeid2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"生活居家\",\"url\":\"/category/getbooklist?columntype=6000&pageid={{page}}&pagesize=20&book_size=-1&typeid=6009&typeid2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"亲子家教\",\"url\":\"/category/getbooklist?columntype=6000&pageid={{page}}&pagesize=20&book_size=-1&typeid=6010&typeid2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"科学技术\",\"url\":\"/category/getbooklist?columntype=6000&pageid={{page}}&pagesize=20&book_size=-1&typeid=6011&typeid2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"教材教辅\",\"url\":\"/category/getbooklist?columntype=6000&pageid={{page}}&pagesize=20&book_size=-1&typeid=6012&typeid2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"童书全部\",\"url\":\"/category/getbooklist?columntype=6000&pageid={{page}}&pagesize=20&book_size=-1&typeid=6013&typeid2=-1&order_type=2&status=-1\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"\",\"url\":\"\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}},{\"title\":\"\",\"url\":\"\",\"style\":{\"layout_flexBasisPercent\":0.25,\"layout_flexGrow\":1}}]",
  "header": "{\n\t\"User-Agent\": \"Mozilla/5.0 (Linux; Android 10; AppleWebKit/537.36 (KHTML, like Gecko) Version/4.0 Chrome/83.0.4103.106 Mobile Safari/537.36\"\n}",
  "lastUpdateTime": 1722185824091,
  "respondTime": 13663,
  "ruleBookInfo": {
    "author": "$.result.author_name@put:{book:$.result.book_id}",
    "coverUrl": "$.result.cover_picture",
    "intro": "🔗 来源：{{$.result.contact_name}}\n{{$.result.book_desc}}##(^|[。！？]+[”」）】]?)##$1<br>",
    "kind": "{{$.result.book_category_full_name##;##,}},{{$.result.book_score}}分,{{$.result.last_update_time##\\s.*}}",
    "lastChapter": "$.result.last_chapter_name##正文卷.|正文.|VIP卷.|默认卷.|卷_|VIP章节.|免费章节.|章节目录.|最新章节.|[\\(（【].*?[求更票谢乐发订合补加架字修Kk].*?[】）\\)]",
    "name": "$.result.book_name",
    "tocUrl": "https://anduril.xmkanshu.com/v3/book/get_last_chapter_list?bookid={{$.result.book_id}}&page=1&pagesize=100000&lastchapterid=0",
    "wordCount": "$.result.book_size##\\."
  },
  "ruleContent": {
    "content": "{{$.result.content}}",
    "start": 2,
    "end": "$.result.pagecount",
    "nextContentUrl": "https://www.xmkanshu.com/service/getContent?fr=smsstg&v=4&uid=B197589CF54DC527538FADCAE6BDBC78&urbid=%2Fbook_95_0&bkid=@get:{book}&crid={{$.result.chapterid}}&pg=@get:{index}"
  },
  "ruleExplore": {
    "author": "$.authorname",
    "bookList": "$.result.books[*]",
    "bookUrl": "https://anduril.xmkanshu.com/v3/book/get_book_info?p13=299940893&p1=H5&p2=PandaBookAndroid5641&p3=e5f3f02806fb7bf1a5ba81c47b97168e&p4=clover&p5=Xiaomi&p6=MI+PAD+4&p7=android&p8=27&p9=wlan&p10=9.3.0.19&p11=1200&p12=1824&p14=93019&p15=10007&p16=4c361b1b778dec5dba77227078270a84&p17=&p18=PandaBookAndroid5641&p19=0&p20=26&book_id={{$.bookid}}&site_id=0",
    "coverUrl": "$.frontcover",
    "intro": "$.bookdesc",
    "kind": "$.booktypename",
    "name": "$.bookname",
    "wordCount": "{{$.booksize}}万字"
  },
  "ruleSearch": {
    "author": "$.author_name",
    "bookList": "$.result.books",
    "bookUrl": "https://anduril.xmkanshu.com/v3/book/get_book_info?p1=H5&p2=PandaBookAndroid5641&book_id={{$.book_id}}&site_id=0",
    "coverUrl": "$.cover_picture",
    "intro": "$.book_desc",
    "kind": "$.book_type_name",
    "name": "$.book_name",
    "wordCount": "$.book_size##\\."
  },
  "ruleToc": {
    "chapterList": "$.result.pageList",
    "chapterName": "$.chapter_name##正文卷.|正文.|VIP卷.|默认卷.|卷_|VIP章节.|免费章节.|章节目录.|最新章节.|[\\(（【].*?[求更票谢乐发订合补加架字修Kk].*?[】）\\)]",
    "chapterUrl": "https://www.xmkanshu.com/service/getContent?fr=smsstg&v=4&uid=B197589CF54DC527538FADCAE6BDBC78&urbid=%2Fbook_95_0&bkid=@get:{book}&crid={{$.chapter_id}}&pg=1",
    "updateTime": "$.createdate"
  },
  "searchUrl": "https://anduril.xmkanshu.com/v3/search/get_result?keyword={{key}}&page_number={{page}}&is_vip_free=0&page_size=20&sub_type=1",
  "weight": 50
}

```
