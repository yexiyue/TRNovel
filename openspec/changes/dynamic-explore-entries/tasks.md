## 1. Source Model 与 Schema

- [x] 1.1 新增 `ExploreEntry`、`EntrySource`（static | fetch 两个 variant）、`StaticEntry`、`FetchEntrySource`、`FetchEntryItem` 等 source model 类型；`ExploreOp.entries` 为 `Vec<EntrySource>`（数组顺序即合并顺序，不引入独立 chain variant）。`FetchEntrySource` 持有一个 `Request`（复用 render/interceptApi/charset/headers）。
- [x] 1.2 复用今天的 `SearchOp` 形状作为共享 `ListPageSpec`，承载 `prelude`、`request`、`list`、`item`；`totalPages`/`hasMore`/`pageBy`/`render`/`interceptApi` 继续挂在 `request: Request` 上（现状，不提到 spec 顶层）。
- [x] 1.3 `SearchOp` 直接裸用 `ListPageSpec`（`pub type` 别名，不包 `page` 层，search JSON 形状不变）；`ExploreOp` 改为 `{ entries: Vec<EntrySource>, page: ListPageSpec }` 两阶段结构。
- [x] 1.4 删除旧 `Category { title, url }` 与旧 `ExploreOp.categories/list/item/render/readyFor/interceptApi/totalPages/hasMore` 结构。
- [x] 1.5 更新 serde/schema 测试，确保旧 explore 格式被拒绝（`old_explore_categories_format_is_rejected`），新 `entries/page` 格式可解析（`parses_explore_entries_and_page`）。

## 2. Entry 加载与规则求值

- [x] 2.1 实现 `Engine::explore_entries().await -> Result<Vec<ExploreEntry>>`，替代同步 `explore_categories()`。
- [x] 2.2 实现 static entry source：title + 字面量 vars 直接映射为 `ExploreEntry`。
- [x] 2.3 实现 fetch entry source：支持 `forEach` 循环变量、请求模板求值、`list` 抽项、`item.title` 与 `item.vars` 求值（ctx=数据项，vars=base+循环变量）。
- [x] 2.4 实现入口源数组合并：按声明顺序遍历各源、拼接产出；动态源失败时保留已成功入口（含静态源），零入口且有报错才返回 Err，不阻断整体加载。
- [x] 2.5 为入口加载添加 per-engine/session 缓存（`entries_cache`），仅完全成功才缓存，失败动态源下次可重试。

## 3. 共享 List Page Runner

- [x] 3.1 抽出 `run_list_page(spec, kind, extra_vars, page, page_size)`，统一处理 prelude、request、render/intercept、page cache、list/item、hasMore、totalPages。
- [x] 3.2 将 `Engine::search` 迁移到共享 list page runner，保持现有搜索行为与点击翻页行为。
- [x] 3.3 将 `Engine::explore` 改为接收 `ExploreEntry`，并通过共享 runner 执行 `explore.page`。
- [x] 3.4 更新 render/intercept 双源路由，确保共享 runner 中 `hasMore` 继续按规则 via 打 body/DOM。
- [x] 3.5 更新 page cache key，纳入 list page 操作类型（'s'/'e'）、entry vars/搜索词、页码与页大小，避免动态入口之间串缓存。

## 4. UI、Doctor 与调用方迁移

- [x] 4.1 更新 `src/pages/network_novel/select_books`，初始化异步加载 explore entries（已在 `use_init_state` 异步闭包内，改 `explore_entries().await?`）并展示加载/错误状态。
- [x] 4.2 将 UI 当前选中项从 `ExploreListItem(Category)` 迁移为 `ExploreListItem(ExploreEntry)`。
- [x] 4.3 更新 `FindBooks` 调用，使用 selected entry 执行 explore 翻页，并保持 search 流程可用。
- [x] 4.4 更新 `trn doctor` / `verify.rs`，浏览测试前先加载动态 entries，并选择首个可用 entry 测试列表。
- [x] 4.5 更新 docs 书源示例（structure/make/rules）中旧 explore 格式引用（README/AGENTS 无旧格式引用）。

## 5. Fanqie Web 书源迁移

- [x] 5.1 将 `fanqie-web.v2.json` 的 explore 改为 `entries` + `page` 新结构。
- [x] 5.2 配置 static entries：「书库·最热」「书库·最新」（`vars: filter=all, sort=hottest|newest`）。
- [x] 5.3 配置 fetch entries：`forEach` gender=0/1 调用 `category_list/v0`，jsonpath 过滤 `label=='主分类'`（女 21 + 男 19 = 40 入口），生成「女/男·<分类名>」入口。
- [x] 5.4 配置 Fanqie entry vars：`filter`（=`audience{gender}-cat2-{category_id}` concat 拼接，只取主分类 → catGroup 恒 2，无需 JS）+ `sort=hottest`；page URL `{{base}}/library/{{filter}}/page_{{page}}?sort={{sort}}`。活体 `trn doctor` 验证返回真书。
- [x] 5.5 确认 Fanqie `page.request` 继续通过 render/intercept 拦截 `book_list/v0`，并保留 `hasMore`、`totalPages`、explore fontMap 处理。

## 6. 测试与验证

- [x] 6.1 添加 source model 解析测试：static/fetch entry source（含 forEach）、新 search/explore page spec、旧格式拒绝。
- [x] 6.2 添加 engine 单元测试：静态入口加载、fetch 入口加载、forEach 合并、entry vars 驱动 page request。
- [x] 6.3 添加共享 runner 回归测试：search 与 explore 都能返回 `BookList { items, has_more, total_pages }`。
- [x] 6.4 添加 Fanqie 配置落地测试：`fanqie-web.v2.json` 可解析并包含动态 entries/page 新结构。
- [x] 6.5 重新生成 `crates/parse-book-source/book-source.schema.json`。
- [x] 6.6 运行 `cargo test --locked --all-features --workspace --lib --tests --examples`。
- [x] 6.7 运行 `cargo clippy --all-targets --all-features --workspace -- -D warnings`。
- [x] 6.8 运行 `cargo fmt --all --check`。
- [x] 6.9 运行 `RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items --all-features --workspace --examples`。
- [x] 6.10 运行 `cargo build`，验证主程序中 `Engine::search/explore` 的 future 仍满足 `tokio::spawn` 的 `Send` 约束。
- [x] 6.11 活体验证：`trn doctor`（动态入口在前的临时配置）端到端通过——浏览/书详情/目录/正文/搜索全 ✓，动态入口「女·女频悬疑」返回 18 本、共 99 页。
