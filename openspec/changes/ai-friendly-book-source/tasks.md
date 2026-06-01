## 1. 脚手架与领域模型(分层骨架)

- [ ] 1.1 在 `crates/parse-book-source/src` 下建立分层模块骨架:`model/ source/ eval/ backend/ fetch/ engine/ verify/ error.rs`(见 design D7)
- [x] 1.2 `model/`:纯领域类型 `Book/BookInfo/Chapter/Volume`(无 IO、无规则逻辑),`Chapter` 含 `is_volume`
- [ ] 1.3 `error.rs`:分层 `thiserror` 错误(`FetchError`/`ExtractError`/`EvalError`/`VerifyError` → 顶层 `BookSourceError`,`#[from]`),`Send+Sync+'static`,**库内不用 anyhow、生产路径无 unwrap/expect**(见 design D10)

## 2. 规则 AST 与求值器(Interpreter + Composite)

- [x] 2.1 `source/rule.rs`:`Rule` 枚举(`Leaf{via,select,index,extract,clean} | FirstOf | Concat{join} | Literal | Template`)+ serde(untagged 判别);v2 配置全套类型 + `bilixs.v2.json` 同构往返/结构测试(9 测试)
- [x] 2.2 `eval/value.rs`、`eval/context.rs`:`Value`(Text/List/Json)与 `Context`(HtmlNode/JsonValue/Text)
- [x] 2.3 `eval/evaluator.rs`:递归下降求值(firstOf 短路、concat join、template 插值、clean 流水线)
- [x] 2.4 单测:各 `via`、各 `extract`、firstOf 回退、concat 拼接、template、clean(regex/trim/prepend)

## 3. 抽取后端(Strategy)

- [x] 3.1 `backend/mod.rs`:`trait ExtractBackend { select / extract }`,**静态分派**(`match via` 调具体实现,闭集,见 D10)
- [x] 3.2 `backend/html.rs`(scraper 或评估 dom_query)、`backend/json.rs`(jsonpath-rust)、`backend/regex.rs`
- [x] 3.3 预留 `backend/xpath.rs` 于 `xpath` feature 之后(本 change 不实现,仅留 `via:"xpath"` 占位)
- [x] 3.4 删除旧 `analyzer/analyzer_manager.rs` 的 DSL 解析(`split_rule_resolve`/`SPLIT_RULE`/`put`/`get`),保留并瘦身底层取值原语

## 4. 取页端口与 HTTP/Cookie(Ports & Adapters)

- [x] 4.1 `fetch/mod.rs`:`trait Fetcher`(`#[async_trait]`,**动态分派 `Arc<dyn Fetcher>`**,见 D10);`fetch/reqwest.rs` 默认实现(reqwest + rustls + cookie_store);时间/退避全用 `tokio::time`,禁 `std::thread::sleep`
- [x] 4.2 `source/http.rs`:结构化 `Http{headers,cookies,charset,timeout,retry,rateLimit}` 与 `Request{url,method,body,headers,vars}`
- [x] 4.3 应用 charset(支持 GBK/gb18030/big5,沿用 encoding_rs)、retry、rateLimit(令牌桶)
- [x] 4.4 Cookie:静态 `http.cookies` 注入 + `http.warmup` 预热 + cookie_store 会话复用
- [ ] 4.5 留出反爬适配位(wreq / FlareSolverr 作为另一 `Fetcher` 实现,属后续 change)

## 5. 用例引擎与分页(Template Method + Paginator)

- [x] 5.1 `engine/paginator.rs`:有界循环组件(`nextPage` 非空则继续、`maxPages` 硬上限封顶、空即停)
- [x] 5.2 `engine/`:`search / explore / book_info / toc / content` 五个用例,复用「取页→选列表/值→映射 item→可选分页」骨架
- [x] 5.3 `EngineBuilder`:组装 `BookSource` + `Fetcher` + 后端(便于注入测试替身);`Engine` 廉价 `Clone`(内部 `Arc`),**操作不跨 await 持锁**(移除旧 `Arc<Mutex<Parser>>` 反模式);大页面解析走 `spawn_blocking`(见 D10)
- [x] 5.4 `toc` 产出扁平章节 + 卷元数据(对齐 `model::Volume`,复用既有 split 思路);`content` 用 paginator

## 6. 验证回路(Samples + 可执行不变量)

- [x] 6.1 `source/sample.rs`:`samples`(bookUrl + expect 不变量)
- [x] 6.2 `verify/`:跑样例 + 各阶段不变量断言 + 结构化失败反馈(断言名/期望/实际/命中 HTML 片段)
- [x] 6.3 运行期自愈:anyOf 候选退化;全候选失效 → 标记书源需修复(不返回脏数据)

## 7. JSON Schema、示例与文档

- [x] 7.1 产出完整 `book-source.schema.json`(供约束解码/校验;`via`/`extract` enum、`clean.regex` pattern)
- [x] 7.2 把 bilixs 改写为 v2 结构化示例 `test-novels/bilixs.v2.json`(含 http/cookies、toc 分卷、samples)
- [x] 7.3 `verify` CLI/示例:对 `bilixs.v2.json` 跑样例,断言 8 卷 / ≥2000 章 / 正文非空(联网)
- [ ] 7.4 文档:v2 书源格式说明 + 字段表 + 「给 AI 的生成指南」(schema + samples 闭环)

## 8. 离线测试与切换

- [x] 8.1 保存 bilixs 各页 fixture(catalog/book/chapter/list),用 mock `Fetcher` 离线单测引擎(不打网络)
- [x] 8.2 全量 `cargo test` + `clippy --all-targets -- -D warnings` + `fmt --check`;`#![deny(missing_docs)]` + 公开 API 文档/doctest;描述式测试名(`x_should_y_when_z`)、一测一断言;domain 不依赖 reqwest/scraper(模块边界断言)
- [x] 8.3 删除旧规则层与旧 `bilixs.json`(紧凑格式),迁移上层(`src/novel/network_novel.rs`)到新 `Engine` API
- [x] 8.4 校验 `openspec validate ai-friendly-book-source --strict`
