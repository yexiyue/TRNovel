## Context

`explore`/`search` 单页 + UI 递增 `page`,但 UI 无「是否还有下一页」信号 → 翻到底空翻 / 报错。真浏览器 spike:番茄 `book_list/v0` 响应 data 顶层有 `has_more`(bool) + `total_count`(实测 10000,占位上限,**≠ 分页器 99 页,不可靠**)。精确总页数番茄只在 DOM 分页器里,而 explore 走 `interceptApi` 拦 API、不读 DOM。

## Goals / Non-Goals

**Goals:** `has_more` 翻页边界——书源 `hasMore` 规则 + 引擎返回 + UI 据此到头停翻。
**Non-Goals:**
- 精确「共 M 页」:`total_count` 不可靠;DOM 分页器要 render DOM,与 explore 的 `interceptApi`(拦 API、无 DOM)冲突,价值低。`has_more` 已满足「翻到头就停」的核心诉求。
- `search` 的 `has_more` 待确认(`search_book/v1` 是否有该字段);可只先给 `explore`。

## Decisions

**D1 — `has_more` 是书源规则(规则即数据)。**
- `ExploreOp`/`SearchOp` 加 `has_more: Option<Rule>`;引擎单页取页后对响应(拦的 API JSON / 渲染 DOM)用该规则求值,**非空且 truthy**(求值结果非 `""`/`"false"`/`"0"`)→ 还有下一页。空规则 = 不提供边界。

**D2 — `explore`/`search` 返回携带 `has_more`。**
- 返回 `BookList { items: Vec<BookListItem>, has_more: Option<bool> }`(或 `(Vec, Option<bool>)`)。**改返回类型**,牵动 `find_book` + doctor + engine 测试。
- 备选(否决):额外方法取 `has_more` —— 要二次取页,浪费;同一响应一并求值最省。

**D3 — 不做精确总页数。**
- `total_count` 实测 10000(占位)、≠ 分页器 99,不可靠;精确页数只在 DOM 分页器,explore 拦 API 无 DOM,为它再 render 一次不划算。

**D4 — UI 边界。**
- `find_book`:`has_more == Some(false)` 时「下一页」键不再 `+page`(到头);底部显示「第 N 页」(当前页,不强求总数)。`None`(无规则)时不限制(现状)。

## Risks / Trade-offs

- [改 `explore`/`search` 返回类型,牵动调用方/测试] → 一次性改 `find_book` + doctor + engine 测试;新结构 `BookList` 清晰。
- [`search_book/v1` 有无 `has_more` 未确认] → 实现前 spike search 响应;无则本 change 只覆盖 `explore`,`search` 留待。
- [`has_more` 求值时机] → 单页取页后对该页响应求值一次,与 `list`/`item` 用同一响应,无额外请求。
