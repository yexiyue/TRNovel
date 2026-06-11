## 1. 配置与返回类型

- [x] 1.1 `source/{op.rs,http.rs}`:`ExploreOp` + `Request`(覆盖 search)加 `has_more: Option<Rule>`(向后兼容,默认无)
- [x] 1.2 `engine`:`BookList`(已由 `render-dual-source` 引入,带 `items`/`total_pages`)additive 加 `has_more: Option<bool>`;`explore`/`search` 单页取页后 `eval_has_more` 求值一次(非空且非 `false`/`0` → `Some(true)`,`false`/`0`/空 → `Some(false)`,无规则/求值失败 → `None`)。**双源路由**:抽 `Rule::primary_via` + `pick_source`,`via:json` 的 has_more 打 API body、`via:css` 的 total_pages 打渲染 DOM,**同会话共存正确**(顺带把 total_pages 从 dom-presence 改为按-via 路由)
- [x] 1.3 schema 重生成(`gen_schema`)+ `schema_is_in_sync` 绿;新增 `has_more_and_total_pages_coexist_via_routing`(双源路由共存)+ `has_more_true_and_none` 两测

## 2. UI 边界

- [x] 2.1 `find_book`:**到头停翻**——`has_more == Some(false)` 或 已达 `total_pages` 时「下一页」键不再 `+page`(双信号有其一即停,都无则不限制=现状)。为让翻页处理读到 books,把 `use_effect_state` 上移到 `use_events` 之前。底部页码「第 N / M 页」由 `render-dual-source` 已落地
- [x] 2.2 其余调用方适配 `BookList`(`render-dual-source` 已改返回类型;本 change 只 additive 加字段,verify/示例无需再改)

## 3. 番茄接入与验证

- [x] 3.1 (spike)`explore` 的 `book_list/v0` 响应 `data.has_more`(bool)已确认(记忆 `fanqie-list-pagination-and-signature`);`search` 的 `search_book/v1` 未单独 spike has_more —— **search 边界由 `total_pages`(分页器,实测 30 页)兜底**,UI 双信号已覆盖,故 search 不强加 hasMore
- [x] 3.2 `fanqie-web.v2.json`:explore 加 `hasMore: {via:json, select:"$.data.has_more"}`
- [x] 3.3 `cargo build` 主程序(`Send` 不回归)+ `test --all-features`(132 passed)+ `clippy -D warnings` + `doc -D warnings` + `fmt`,全绿
- [ ] 3.4 UI 实跑:番茄书库 explore / search 翻到最后一页后「下一页」键不再前进(**待用户浏览器环境**)
