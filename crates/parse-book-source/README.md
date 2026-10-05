# parse-book-source

TRNovel 的结构化 v2 书源引擎，支持搜索、发现、书籍详情、目录和正文提取，以及 CSS、XPath、JSONPath、正则和原生清洗算子。当前格式为 `trnovel-booksource/v2`，不直接兼容 Legado 书源 JSON。

运行入口是 `Engine`，配置通过 `BookSource::from_json` 加载。使用说明和配置示例见：

- [书源介绍](https://yexiyue.github.io/TRNovel/book-source/intro)
- [规则语法](https://yexiyue.github.io/TRNovel/book-source/rules)
- [JSON Schema](book-source.schema.json)
- [浏览器搜索示例](examples/engine_search_poc.rs)

在仓库根目录生成 Schema：

```bash
cargo run -p parse-book-source --features schema --example gen_schema
```

生成或导入书源后，用 `trn doctor <source.v2.json>` 校验。浏览器相关示例需要 `browser` feature 和系统浏览器。
