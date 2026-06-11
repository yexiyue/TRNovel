## 1. explore 渲染通道(source)

- [x] 1.1 `source/op.rs`:`ExploreOp` 内联 `render`/`readyFor`/`interceptApi`(对齐 `Request`、分类间共享;`Category` 保持 `{title, url}`,url 模板带 `{{page}}`)。遵循 `BookInfoOp` 内联先例(serde `flatten` 与 `deny_unknown_fields` 不兼容)
- [x] 1.2 `source/mod.rs` 测试:解析 `explore.render`/`interceptApi`、round-trip 相等、`deny_unknown_fields` 拒未知字段、无字段时序列化逐字节不变(向后兼容)

## 2. engine 单页渲染路由

- [x] 2.1 `engine.explore` 取页按 `op.render` 走渲染/CDP 拦截或 `fetch_checked`(与 `search` 同款路由);`explore`/`search` 均为**单页普通 async fn**(无 async 闭包 → Future 仍 `Send`,主程序 `tokio::spawn` 依赖之)。翻页由 UI 递增入参 `page` 驱动
- [x] 2.2 `engine/tests.rs`:explore 开 `interceptApi` 走渲染(`FetchRequest.render==true`)、未开保持 `fetch_checked`、渲染失败优雅降级、单页只取一次

## 3. 番茄书源接入

- [x] 3.1 `fanqie-web.v2.json`:`explore` 切书库——分类 url `{{base}}/library/all/page_{{page}}?sort=hottest|newest`、`render:true` + `interceptApi:"book_list/v0"`、`list`/`item` 用 `via:json`(`$.data.book_list[*]`、`book_id`→`bookUrl`);单页,UI 递增 `page` 翻页
- [ ] 3.2 用 `trn doctor` / UI 实跑确认番茄书库浏览能出数据、递增 `page` 能翻到下一页 — ⏸ **待用户在「浏览器 + 番茄登录」环境实跑**(沙箱无法真渲染)

## 4. schema 与文档

- [x] 4.1 重生成 `crates/parse-book-source/book-source.schema.json`(`--features schema`)并通过 `schema_is_in_sync` 防漂移测试
- [x] 4.2 `booksource-generator` skill 文档:补 explore 渲染取页写法 + `page` 基数对齐约定(URL `page_{{page}}` 从 1、API `page_index` 从 0,由书源 URL 模板对齐,引擎不内置偏移)
- [x] 4.3 design 固化:`by:url` 引擎批量翻页的回退原因(D1,与 UI 单页递增冲突)、不破解/不搬 boa 签名(D4 实测);动态总页数 / 番茄 search 翻页 / 浏览器池化列为 Non-Goals + Open Questions 的后续 change
- [x] 4.4 全量验证:`cargo build`(主程序 trnovel,验 `Send`)+ `test --all-features` + `clippy -D warnings` + `doc(RUSTDOCFLAGS=-D warnings)` + `fmt --check`,全绿
