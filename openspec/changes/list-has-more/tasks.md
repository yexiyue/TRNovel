## 1. 配置与返回类型

- [ ] 1.1 `source/op.rs`:`ExploreOp`/`SearchOp` 加 `has_more: Option<Rule>`(向后兼容,默认无)
- [ ] 1.2 `engine`:`explore`/`search` 返回携带 `has_more`(新结构 `BookList { items, has_more }` 或元组);单页取页后对该页响应用 `hasMore` 规则求值一次(非空且非 `false`/`0` → `Some(true)`),无规则 → `None`
- [ ] 1.3 schema 重生成 + `source` 测试(解析 / round-trip / `deny_unknown_fields` / 向后兼容)

## 2. UI 边界

- [ ] 2.1 `find_book`:`has_more == Some(false)` 时「下一页」键到头不再 `+page`;底部显示「第 N 页」(当前页)
- [ ] 2.2 其余调用方 / doctor 适配新返回类型

## 3. 番茄接入与验证

- [ ] 3.1 (前置 spike)实跑确认 `search_book/v1` 响应是否有 `has_more`;有则 `search` 也加,无则本 change 只覆盖 `explore`
- [ ] 3.2 `fanqie-web.v2.json`:explore 加 `hasMore: {via:json, select:"$.data.has_more"}`
- [ ] 3.3 `cargo build` 主程序 + `test --all-features` + `clippy -D warnings` + `doc` + `fmt`,全绿;UI 实跑确认翻到头停翻(待用户浏览器环境)
