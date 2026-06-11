## Why

`explore`/`search` 翻页目前(配合 `list-has-more`)只有 `has_more`(还有没有下一页),给不出「第 N / **M** 页」的总数 —— 用户翻 99 页的书库时没有进度感。

`list-has-more` 的 D3 把精确总页数列为 Non-Goal,理由是「explore 走 `interceptApi` 拦 API、无 DOM,总页数只在 DOM 分页器里,为它再 render 一次不划算」。**这个前提是错的**:`interceptApi` 本来就驱动一张**真实浏览器页面**渲染(`intercept_page` 拿的就是活页面),分页器的 99 就在那张页的 DOM 里 —— 根本不用「再 render 一次」,只是当前把 DOM 那一份**扔掉了**(只回了拦到的 API JSON)。

真正的卡点是**数据分在两个源**:列表/`has_more` 来自拦截的 API JSON(`via:json`),总页数来自 DOM 分页器(`via:css`)。要取总页数,render 层必须把渲染后的 DOM 也带出来。

## What Changes

把这件事做成**通用能力**(而非只为总页数特判):

- **render(intercept)同时暴露两个源**:拦到 API JSON 之外,(可选地)抓渲染后的 DOM(`outerHTML`)。规则按 `via` 路由 —— `via:json` 求值 API JSON、`via:css`/`via:xpath` 求值 DOM。DOM 因此成为 css 可寻址的源,不止总页数,DOM 里任何东西都能取。
- **`totalPages` 通用规则**:`explore`/`search`(及底层 `Request`)增可选 `totalPages: Option<Rule>`,对取页结果求值得「共 M 页」。`via:json`(API 总数靠谱的源)/ 非 render 书源 `via:css`(reqwest HTML)**零基建白送**;fanqie 这类「总数只在 render DOM」的走 `via:css` + 上面的双源能力。
- **时序闸**:分页器在 API 之后才渲染,故抓 DOM 前**等就绪选择器**出现(放宽现有 `intercept_api`/`ready_for` 的「二选一」为「可同时给」:intercept 取数 + ready_for 作 DOM 就绪闸)。
- **返回携带总数**:`explore`/`search` 返回的 `BookList` 增 `total_pages: Option<u32>`(与 `list-has-more` 的 `has_more` 同结构,additive)。
- **UI**:`find_book` 底部「第 N 页」→「第 N / M 页」(有总数时);无 `totalPages` = `None`,退化为现状(只显示当前页)。
- 向后兼容:无 `totalPages` 规则 = 不抓 DOM、不返回总数 = 现状;非 render / 旧书源不受影响。

## Capabilities

### New Capabilities
- `render-dual-source`: render 拦截取页把「拦到的 API JSON」与「渲染后的 DOM」暴露成两个 `via` 可寻址的源,规则各取所需;首个消费者是 `explore`/`search` 的 `totalPages`(精确总页数),但能力通用(DOM 里任意字段)。

## Impact

- `crates/parse-book-source/src/fetch/mod.rs`:`FetchResponse` 增 `dom_html: Option<String>`(render+intercept 抓到的渲染 DOM;其它路径为 `None`)。
- `crates/parse-book-source/src/fetch/browser/fetcher.rs`:`render_intercept` 在拦到 API body 后,(配了 DOM 源规则时)等就绪选择器、抓 `outerHTML` 一并返回;`EscalatingFetcher` 透传。
- `crates/parse-book-source/src/source/{op.rs,http.rs}`:`ExploreOp`/`SearchOp`/`Request` 增 `total_pages: Option<Rule>`;放宽 `intercept_api`+`ready_for` 可共存。
- `crates/parse-book-source/src/engine/`:规则求值按 `via` 路由到 API body / DOM 源;`explore`/`search` 返回的 `BookList` 增 `total_pages`。
- `crates/parse-book-source/book-source.schema.json`:随类型重生成。
- `src/pages/network_novel/select_books/find_book.rs`:底部页码改「第 N / M 页」。
- 书源数据:`fanqie-web.v2.json` explore 加 `totalPages`(`via:css` 选分页器)。
- `openspec/changes/list-has-more/design.md`:D3 改正(其「无 DOM」前提作废,指向本 change)。
- 风险:`FetchResponse` 加 render 概念字段;DOM 体积(只为读一个数字 ship 整页 HTML → 仅在配了 DOM 源规则时才抓)。(fanqie 分页器选择器、search 是否有总数已 spike 实测确认 —— `byte-pagination` 组件,explore + search 通吃,见 design。)
