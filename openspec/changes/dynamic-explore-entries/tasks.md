## 1. Source Model 与 Schema

- [ ] 1.1 新增 `ExploreEntry`、`EntrySource`、`StaticEntrySource`、`FetchEntrySource`、`FetchEntryItem` 等 source model 类型。
- [ ] 1.2 新增共享 `ListPageSpec`，承载 `prelude`、`request`、`list`、`item`、`totalPages`、`hasMore`。
- [ ] 1.3 将 `SearchOp` 改为使用 `ListPageSpec`，将 `ExploreOp` 改为 `{ entries, page }` 两阶段结构。
- [ ] 1.4 删除旧 `Category { title, url }` 与旧 `ExploreOp.categories/list/item/render/readyFor/interceptApi/totalPages/hasMore` 结构。
- [ ] 1.5 更新 serde/schema 测试，确保旧 explore 格式被拒绝，新 `entries/page` 格式可解析。

## 2. Entry 加载与规则求值

- [ ] 2.1 实现 `Engine::explore_entries().await -> Result<Vec<ExploreEntry>>`，替代同步 `explore_categories()`。
- [ ] 2.2 实现 static entry source：按规则求值 title 与 vars，生成 `ExploreEntry`。
- [ ] 2.3 实现 fetch entry source：支持 `forEach` 循环变量、请求模板求值、`list` 抽项、`item.title` 与 `item.vars` 求值。
- [ ] 2.4 实现 chain entry source：按顺序合并子源结果，并明确动态子源失败时的错误传播与已成功入口保留策略。
- [ ] 2.5 为入口加载添加 per-engine/session 缓存，避免同一书源重复请求动态分类 API。

## 3. 共享 List Page Runner

- [ ] 3.1 抽出 `run_list_page(spec, vars, page, page_size)`，统一处理 prelude、request、render/intercept、page cache、list/item、hasMore、totalPages。
- [ ] 3.2 将 `Engine::search` 迁移到共享 list page runner，保持现有搜索行为与点击翻页行为。
- [ ] 3.3 将 `Engine::explore` 改为接收 `ExploreEntry` 或 entry vars，并通过共享 runner 执行 `explore.page`。
- [ ] 3.4 更新 render/intercept 双源路由，确保共享 runner 中 `hasMore` 继续按规则 via 打 body/DOM。
- [ ] 3.5 更新 page cache key，纳入 list page 操作类型、entry vars、搜索词、页码与页大小，避免动态入口之间串缓存。

## 4. UI、Doctor 与调用方迁移

- [ ] 4.1 更新 `src/pages/network_novel/select_books`，初始化时异步加载 explore entries 并展示加载/错误状态。
- [ ] 4.2 将 UI 当前选中项从 `ExploreListItem(Category)` 迁移为 `ExploreListItem(ExploreEntry)`。
- [ ] 4.3 更新 `FindBooks` 调用，使用 selected entry 执行 explore 翻页，并保持 search 流程可用。
- [ ] 4.4 更新 `trn doctor` / `verify.rs`，浏览测试前先加载动态 entries，并选择首个可用 entry 测试列表。
- [ ] 4.5 更新 parse-book-source README、docs 书源示例和 AGENTS/CLAUDE 相关说明中旧 explore 格式引用。

## 5. Fanqie Web 书源迁移

- [ ] 5.1 将 `fanqie-web.v2.json` 的 explore 改为 `entries` + `page` 新结构。
- [ ] 5.2 配置 static entries：至少包含「全部·最热」与「全部·最新」。
- [ ] 5.3 配置 fetch entries：对 `gender=0` / `gender=1` 调用 `category_list/v0`，生成男生/女生分类入口。
- [ ] 5.4 配置 Fanqie entry vars：生成 `filter`、`audienceToken`、`catGroup`、`categoryId`、`sort=hottes` 等取页变量。
- [ ] 5.5 确认 Fanqie `page.request` 继续通过 render/intercept 拦截 `book_list/v0`，并保留 `hasMore`、`totalPages`、fontMap 处理。

## 6. 测试与验证

- [ ] 6.1 添加 source model 解析测试：static/fetch/chain entry source、新 search/explore page spec、旧格式拒绝。
- [ ] 6.2 添加 engine 单元测试：静态入口加载、fetch 入口加载、forEach 合并、entry vars 驱动 page request。
- [ ] 6.3 添加共享 runner 回归测试：search 与 explore 都能返回 `BookList { items, has_more, total_pages }`。
- [ ] 6.4 添加 Fanqie 配置落地测试：`fanqie-web.v2.json` 可解析并包含动态 entries/page 新结构。
- [ ] 6.5 重新生成 `crates/parse-book-source/book-source.schema.json`。
- [ ] 6.6 运行 `cargo test --locked --all-features --workspace --lib --tests --examples`。
- [ ] 6.7 运行 `cargo clippy --all-targets --all-features --workspace -- -D warnings`。
- [ ] 6.8 运行 `cargo fmt --all --check`。
- [ ] 6.9 运行 `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items --all-features --workspace --examples`。
- [ ] 6.10 运行 `cargo build`，验证主程序中 `Engine::search/explore` 的 future 仍满足 `tokio::spawn` 的 `Send` 约束。
