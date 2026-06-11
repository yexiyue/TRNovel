## Context

`render-fetcher` 让 `search` 主请求能渲染 SPA / CDP 拦截签名 API,接通了番茄网页搜索第一页。但 `explore` 的渲染没接通:`render`/`readyFor`/`interceptApi` 只挂在 `Request` 上,`explore` 用 `Category{title,url}` + `engine.explore` 的 `fetch_checked`(纯 reqwest),SPA 站点(番茄书库)的浏览/分类列表整页拿不到(reqwest 只得空壳)。

**翻页本身不是引擎的事**:UI(`find_book.rs`)用「单页 + 用户按键递增 `page`」翻页,`explore(category, page)` 只需取第 `page` 页(`{{page}}` 映射 URL)。番茄书库之前翻不了,根因是 explore 取不到 SPA 数据,而非缺分页机制。

真浏览器 spike(`agent-browser` 实跑 `fanqienovel.com`)的翻页机制证据(保留备查,指导后续 change):

```
explore 书库 /library —— URL 驱动
  分页器 byte-pagination,共 99 页;直接导航 /library/all/page_N → SPA 自动渲染第 N 页
  API: /api/author/library/book_list/v0/?page_count=18&page_index=N-1&...&a_bogus=<签名>

search /search/{词} —— 点击驱动(URL 不认页码)
  分页器共 30 页(300 本);URL 不变,点分页器才令 search_book page_index 递增
  /search/{词}/page_2、?page_index=1、?page=2 实测均回第 1 页
  API: /api/author/search/search_book/v1?...&page_count=10&page_index=N-1&query_word=<词>&a_bogus=<签名>
```

签名 `a_bogus`(随 page 变)/ `msToken`(会话级)绑浏览器上下文、不可外带复刻(`render-fetcher` 铁律),番茄列表只能让真浏览器自己渲染/签名。约束:纯 serde 配置、规则即数据;向后兼容逐字节;渲染仅 `browser` feature、失败优雅降级。

## Goals / Non-Goals

**Goals:**
- 给 `explore` 开渲染取页通道(对齐 `search` 的 `render`/`readyFor`/`interceptApi`),使番茄书库等 SPA 浏览列表可取。
- `explore`/`search` 保持**单页**(`{{page}}` 映射 URL),交互翻页由 UI 递增 `page` 驱动。
- 番茄书库 explore 接入(单页 render)。
- 默认零行为变化(不开 `render` = 现状 reqwest)。

**Non-Goals:**
- **不在引擎做批量翻页**:曾试的 `by:url`(引擎一次翻 `maxPages` 页累积)与 UI「单页 + 递增 page」模型冲突——一进书库连开 N 个浏览器、内容重复,已回退(见 D1)。
- **动态总页数**(从分页器/API 读总页数给 UI 显示边界):正确的翻页增强,牵动 explore/search 返回类型 + UI,留**独立 change**(需先 spike `book_list/v0` 响应有无 `total`)。
- 番茄 **search 翻页**(URL 不认页码,需点击驱动):留独立 change。
- **浏览器实例池化**(render explore 每页开一次浏览器,固有成本):留后续优化。

## Decisions

**D1 — 翻页是 UI 的事;引擎只做单页 + explore 渲染通道(A)。曾试的 by:url 已回退。**
- UI 唯一调用点 `find_book.rs` 是「单页 + 用户递增 page」;`explore(category, page)` / `search(key, page)` 只取第 `page` 页(`{{page}}` 映射 URL,如 `/library/all/page_{{page}}`)。交互翻页由 UI 递增 `page` 重新调用驱动。
- 曾引入 `by:url`(`nextPage` 配置 + 引擎一次翻 `maxPages` 页累积),实测与 UI 模型**正面冲突**:`explore(cat, 1)` 一次翻 5 页连开 5 个浏览器、`explore(cat, 2)` 又翻 2..6 重复,`maxPages` 还得写死。**在整个 TRNovel 无正确用例**(UI 自己管翻页),故回退——删 `NextPage`/`NextPageBy`、`SearchOp`/`ExploreOp.next_page`、引擎分页循环。
- 真正缺的只是 A(explore 能 render);加上后,单页 + UI 递增 `page` 自然翻页,且一次只开一个浏览器。

**D2 — explore 渲染字段内联到 `ExploreOp`(`BookInfoOp` 先例)。**
- `ExploreOp` 内联 `render`/`readyFor`/`interceptApi`(与 `Request` 同名同序),分类间共享;`Category` 保持 `{title,url}`,`url` 模板带 `{{page}}`。
- serde `flatten` 与 `deny_unknown_fields` 不兼容,故不抽共用结构 flatten;遵循 `BookInfoOp`(重复 `BookRules` 字段而非 flatten)先例内联,用测试保证与 `Request` 不漂移。

**D3 — 番茄书库 URL 驱动(spike 实测)。**
- explore 书库:`/library/all/page_N` → SPA 渲染第 N 页,拦 `book_list/v0?page_index=N-1`;URL `page_{{page}}` 从 1、API `page_index` 从 0,由书源 URL 模板对齐,引擎不内置偏移。
- `fanqie-web.v2.json` explore:`{{base}}/library/all/page_{{page}}?sort=hottest|newest` + `render:true` + `interceptApi:book_list/v0` + `list/item` 用 `via:json`(`$.data.book_list[*]`、`book_id`→`bookUrl`)。

**D4 — 不破解 / 不复刻签名,不把签名 JS 搬进 boa(已实测否决)。**
番茄列表 API 带 `a_bogus`(请求签名,**随 `page_index` 变**)+ `msToken`(**会话级,不随 page 变**,实测翻页两条请求 msToken 完全相同)。三条"绕开 render、纯算法签名"的路都走死:
- **纯 Rust 复刻 `a_bogus`**:ByteDance secsdk 的 VMP 混淆 + 远程下发策略(`config_127`/`project_127`/`strategy_127`),≈不可能且隔月即碎。
- **把签名 JS 搬进 boa**:实测 `secsdk-lastest.umd.js`(190KB)/`bdms.js`(238KB)依赖 `document`(40/21 次)、`window`(33/26)、`navigator`(2/16)、`Worker`;boa 是纯 ECMAScript 引擎(无 DOM/BOM/Worker),一加载即报错。
- **boa + mock 浏览器环境**:secsdk 会检测环境真实性,假环境被识别即拒签或返回无效签名,还要持续追字节改版。

故 `render`(让真浏览器 + 真 SPA 自己签、CDP 拦响应)是唯一稳的路线——这也正是 `render-fetcher` 立「MUST NOT 复刻任何站点签名」铁律的根因。boa 的定位是**确定性纯计算逃生舱**(md5/aes/base64,无环境依赖),不是浏览器模拟器。

## Risks / Trade-offs

- [render explore 每翻一页开一次浏览器,秒级] → SPA 固有成本;低频浏览可接受;**浏览器实例池化**留后续优化。
- [番茄 search 翻页拿不到(URL 不认页码)] → 需点击驱动,留独立 change;第一页 10 本足够「定位到书」,海量浏览走 explore 书库。
- [动态总页数缺失,UI 不知道共几页 / 到头没边界] → 留独立 change(从 `book_list/v0`/`search_book/v1` 响应 `total`/`page_count` 读,或读渲染后 DOM 的 `byte-pagination` 最大页码),需先 spike API 响应结构。
- [自动化指纹被 secsdk 拒签致空] → 沿用 `render-fetcher` 失败降级。

## Migration Plan

- 纯增量:`explore` 新增可选 `render`/`readyFor`/`interceptApi`,旧书源不动、行为不变。
- 落地验证:`fanqie-web.v2.json` explore 单页 render;`trn doctor` / UI 确认番茄书库浏览能出数据、UI 递增 `page` 翻页(一次只开一个浏览器)。
- 回滚:移除 `explore.render*` 即回单页 reqwest(无数据迁移)。

## Open Questions

- **动态总页数**(用户提出,后续 change):`explore`/`search` 除书列表外返回总页数,UI 显示「第 N / 共 M 页」、到头不让翻。从 `book_list/v0`/`search_book/v1` 响应的 `total`/`page_count` 读(需 spike 确认有此字段),或读渲染后 DOM 的 `byte-pagination` 最大页码。牵动返回类型 + UI,单独立项。
- **浏览器池化**:render explore 每页开浏览器慢,是否池化/复用实例以顺滑翻页。
- `page` 基数对齐(URL `page_{{page}}` 从 1、API `page_index` 从 0,书源 URL 模板对齐、引擎不内置偏移)已写入 `booksource-generator` 文档。
