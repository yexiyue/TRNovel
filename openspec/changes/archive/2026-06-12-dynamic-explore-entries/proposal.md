## Why

番茄 Web 的书库入口来自站点数据与 API 驱动的筛选状态，但 TRNovel 目前仍把 explore 入口建模成手写 `Vec<Category>`，入口本身等同于静态 URL。动态分类入口会成为后续书源的主推方向，因此需要把 explore 架构重建为「动态生成入口 + 统一列表取页」，而不是在旧结构上继续补字段。

## What Changes

- **BREAKING**：移除旧 explore 结构（`categories` 加 `list`/`item`/`render` 等字段），改为两阶段模型：入口加载与列表取页。
- 新增动态 explore entry：可请求远端分类数据，按规则抽取并平铺成 UI 可选择的入口列表。
- explore entry 携带 `title` 与变量集合，不再把 URL 作为入口的核心身份；取页 URL 由统一列表页规则用 entry 变量和 `page` 生成。
- 引入 search/explore 共用的列表页规格，统一 `request`、`render`、`interceptApi`、`list`、`item`、`hasMore`、`totalPages` 等逻辑。
- 将 `fanqie-web.v2.json` 迁移为从 `category_list/v0` 动态生成入口，并继续用现有 render/intercept 的 `book_list/v0` 路线抓取书籍列表。
- 更新 schema、示例、doctor、测试和 UI 初始化流程，让入口加载成为异步能力。
- 不兼容旧外部书源 explore 格式；仓库内书源与 schema 一起迁移到新结构。

## Capabilities

### New Capabilities

- `dynamic-explore-entries`：定义动态 explore 入口加载、平铺转换、入口变量传递，以及 search/explore 共享列表页取数行为。

### Modified Capabilities

无。相关的 render 列表分页、`hasMore`、`totalPages` 行为会被纳入新的共享列表页规格中描述，不再为仍处于 active change 的旧能力单独维护 delta spec。

## Impact

- `crates/parse-book-source/src/source/*`：重建 explore/source schema 与模型，引入 entry source 和共享 list page spec。
- `crates/parse-book-source/src/engine/*`：实现异步入口加载、入口缓存、共享列表页 runner，以及 Fanqie 动态入口测试。
- `src/pages/network_novel/select_books/*`：异步加载 explore entries，并用选中 entry 的变量执行 explore 翻页。
- `crates/parse-book-source/src/verify.rs`：doctor 在测试 explore 前先加载动态入口。
- `fanqie-web.v2.json`：迁移为基于 Fanqie `category_list/v0` 的动态入口源。
- `book-source.schema.json`、docs 与示例：更新为新格式，删除旧 explore 结构示例。
