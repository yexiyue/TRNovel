## Why

`explore`/`search` 单页翻页(UI 递增 `page`)目前**没有边界信号**:UI 不知道还有没有下一页,用户翻到底会拿到空列表 / 报错,体验差。

真浏览器 spike:番茄 `book_list/v0` 响应 data 顶层有 **`has_more`**(bool)——直接告诉"还有没有下一页";而 `total_count` 是占位上限(实测 10000,与分页器实际 99 页对不上,**不可靠**),精确"共 M 页"番茄 API 不给(只在 DOM 分页器里,explore 拦 API 时不读 DOM)。故用 `has_more` 做翻页边界,而非精确总页数。

## What Changes

- **`explore`/`search` 增可选 `hasMore` 规则**:对取页响应(拦截的 API JSON 或渲染 DOM)求值得「是否还有下一页」(如 `$.data.has_more`);为空 = 不提供边界(现状,UI 不限制)。
- **`engine.explore`/`search` 返回携带 `has_more`**:返回类型从 `Vec<BookListItem>` 改为携带书列表 + `Option<bool> has_more`(如 `BookList { items, has_more }`)。
- **UI 用边界**:`find_book` 据 `has_more` 决定「下一页」键是否到头(`false` 不再 `+page`);底部显示「第 N 页」。
- 向后兼容:无 `hasMore` 规则 = 不返回边界 = UI 不限制(现状);非 SPA / 旧书源不受影响。

## Capabilities

### New Capabilities
- `list-has-more`: `explore`/`search` 翻页边界——书源声明 `hasMore` 规则对响应求值,引擎返回「是否还有下一页」,UI 据此到头停翻、显示当前页码。

## Impact

- `crates/parse-book-source/src/source/op.rs`:`ExploreOp`/`SearchOp` 增可选 `has_more: Option<Rule>`。
- `crates/parse-book-source/src/engine/`:`explore`/`search` 返回类型携带 `has_more`(新结构或元组);单页取页后对响应求值一次。
- `crates/parse-book-source/book-source.schema.json`:随类型重生成。
- `src/pages/network_novel/select_books/find_book.rs`:翻页键据 `has_more` 加边界 + 底部页码展示。
- 书源数据:`fanqie-web.v2.json` explore 加 `hasMore: $.data.has_more`。
- 风险:改 `explore`/`search` 返回类型,牵动所有调用方与测试(doctor/UI);`search`(`search_book/v1`)是否有 `has_more` 待确认(可只先给 explore)。
