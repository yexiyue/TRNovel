## Why

`render-list-pagination` 给 explore 开了渲染取页,`render-dual-source` 让 search 能显示「第 N / M 页」,但 **search 的翻页本身是坏的**:番茄 search 主请求 URL 模板是 `{{base}}/search/{{key}}`(**不带 `{{page}}`**),UI 递增 `page` 重新调用 `search(key, N)` 时导航的是同一个 URL,SPA 初始请求恒为 `page_index=0` → **每一页都返回重复的第 1 页 10 本**;而分页器 DOM 读出的 `totalPages=30` 又 advertise 了 29 个根本翻不到的页,是个**会误导用户的活 bug**。

真浏览器 spike(agent-browser 实测 + 独立复核,2026-06-11)确证:番茄 search 翻页**只能点击驱动**——6 种直达 URL 变体(`/page_2`=404、`?page_index=1`、`?page=2`、`?page_index=1&page_count=10`、`#/page/2`、`?p=2&offset=10`)**全部失败**(5 个静默回第 1 页、`/page_2` 是 404),分页态只活在 React 内存 + 签名的 `page_index=N` API 调用里,地址栏永不变。`a_bogus`/`msToken` 每请求重签、绑浏览器上下文不可复刻(`render-fetcher` 铁律),所以唯一正确路线是**让真 SPA 自己点分页器翻页、CDP 拦每页 API 响应**。

## What Changes

- **新增点击驱动翻页配置 `pageBy`**:`Request`(覆盖 search 主请求及任何 render 拦截源)新增可选 `pageBy: { click: "<next 选择器>" }`。配了且 `page > 1` 时,引擎让受控浏览器在**一次活页**里点「下一页」`page-1` 次、拦截**第 `page` 页**的 API 响应作为该页结果;不配则现状(单拦截 / `{{page}}` URL 模板翻页),翻页行为逐字节不变。
- **新增浏览器原语 `render_intercept_paged`**:在现有 `with_pool_page` 闭包内驱动(活页只在本次调用内存活,**不引入跨调用会话状态**),内含 spike 实测必须的四道健壮性:
  - ① **稳健点击投递 + 前进确认**:`scrollIntoView` + 派发真实 `MouseEvent`(实测离屏 CLI 单击无效、需真事件);**以「点击之后到达的、URL 含 `interceptApi` 的新响应」为页已前进的判据**(不依赖 byte-pagination 特定 DOM,保通用);**不假设「一次点击=翻一页」**,未前进则重试 / 超时终止。
  - ② **响应相关性(强制)**:只认 click *之后*到达、且按**目标 `page_index`(= `page-1`,从响应 URL query 解析)对齐**的响应,**避开 reload 残留的 `page_index=0` 旧响应**(D5 的 reload 会注入它,纯端点 substring 匹配会误采)。
  - ③ **软封锁 reload-once 恢复**:撞空 body / `.muye-search-empty`「共 0 项」/ `verify.zijieapi.com` 滑块 iframe → reload 一次再判零结果。
  - ④ **page-N DOM 落定**:点击后 DOM 异步于网络响应更新(实测 ~1s),抓第 N 页 `outerHTML`(供 `render-dual-source` 的 `via:css` totalPages)前**等 DOM 落定**(active 页文本 == 目标 / 重等 `readyFor`),否则抓到 stale 分页器、读错总页数。
- **软封锁恢复触达所有 render+intercept 首页路径(有意的严格改善)**:③的 reload-once 同样接入现有 `render_intercept` 首页路径——现状 search 第 1 页本身也会偶发撞软封锁、被现状代码误报「0 结果 / 未拦截到」。空 body 本就是失败态,reload-once 严格改善、**绝不改变原本成功的结果**;但这意味着「`pageBy` 缺席 = 零行为变化」需收窄为「**翻页行为**不变」(见下「向后兼容」)。
- **番茄接入**:`fanqie-web.v2.json` 的 `search.request` 加 `pageBy.click`(实测稳健选择器 `.byte-pagination .byte-pagination-item-icon:has(.byte-icon-right)`)。explore(`/library` URL 驱动、by:url 已可翻)**不接** `pageBy`,保持现状。
- **向后兼容**:`pageBy` 缺席 = **翻页行为**与本 change 前逐字节一致(单拦截 / `{{page}}` URL 模板);唯一例外是软封锁 reload-once 恢复会作用于所有 render+intercept 取页(含不配 pageBy 的第 1 页),这是对「空 body 误报」的有意修复(严格改善失败态)。全部仅在已有 `browser` feature 门控下编译,渲染失败优雅降级。

## Capabilities

### New Capabilities
- `search-click-pagination`: render 拦截型取页在「SPA URL 不认页码、翻页只能点分页器」时的点击驱动翻页能力——书源以 `pageBy.click` 声明「下一页」选择器,引擎在一次活页内点 `page-1` 次并拦截目标页 API 响应,含稳健点击投递、`page_index` 相关性、到头停翻、软封锁 reload-once 恢复、page-N DOM 落定;`pageBy` 缺席时保持单页取页(现状),不影响 by:url 翻页与非 render 路径。

### Modified Capabilities
<!--
  Modified Capabilities = none(语义裁决,非归档技术性):click-pagination 在一张活页内点击到第 N 页、**只返回第 N 页、只开一个浏览器、不累积多页**,完全满足 render-list-pagination「引擎 MUST NOT 自行批量翻多页」的规范约束(该 MUST NOT 针对的是"累积 + 连开多浏览器",二者本 change 都不做)。它只是**新增**了 URL 模板 `{{page}}` 映射之外的另一种单页取页机制(点击到第 N 页),单页语义不变,故为 additive、不构成对已归档/在研 spec 的需求变更。详见 design D1/D2。
-->

## Impact

- `crates/parse-book-source/src/fetch/browser/fetcher.rs`:新增 `render_intercept_paged`(点击循环 + 四道健壮性);**`intercept_body` 须拆**为「arm 监听一次 + 等下一个匹配响应」两段(EventStream 跨点击循环持有),供点击后等待复用;抽「派发真实点击 + 按 page_index 等点击后响应」「软封锁判定 + reload-once」「DOM 落定」助手。
- `crates/parse-book-source/src/fetch/mod.rs`:`FetchRequest` 承载点击翻页所需(`page_by` 选择器 + 目标页 `page`);仅 render+intercept 路径用,默认无(`page` 须在 escalating 路由判定 `req.page>1` 前存在)。
- `crates/parse-book-source/src/fetch/browser/escalating.rs`:render 分支在 `intercept_api` 且有 `page_by` 且 `page>1` 时改走 `render_intercept_paged`,否则现状 `render_intercept`;外层 `RENDER_RETRY` 会重跑整段 paged 原语(N-1 次点击重来)——N≤5 可接受,注明。
- `crates/parse-book-source/src/source/http.rs`:`Request` 新增 `pageBy: Option<PageBy>`(`{ click: String }`,camelCase、`skip_serializing_if` none、与 `Retry`/`RateLimit` 等同款 `#[serde(rename_all="camelCase", deny_unknown_fields)]` + schema cfg_attr;内联贴 `render`/`readyFor`/`interceptApi` 先例,Option+default 不破 `deny_unknown_fields` 向后兼容)。
- `crates/parse-book-source/src/engine/mod.rs` + `internal.rs`:`search` 把 `pageBy` + `page` 透传进 `send_templated_full`/`FetchRequest`;`explore` 不变(URL 驱动)。**`search` 仍是普通 `async fn`、无 async 闭包**,保 `tokio::spawn` 所需 `Send`(主程序 `cargo build` 验证)。
- `crates/parse-book-source/book-source.schema.json`:随 `PageBy` 类型重生成(`--features schema`);`schema_is_in_sync` 是**强制 gate**,不重生成即测试 FAIL。
- 书源数据:`fanqie-web.v2.json` `search.request` 加 `pageBy.click`;`booksource-generator` skill 文档补「click 驱动翻页 + 选择器/软封锁约定」。
- 风险:① 点击投递在 secsdk 重型 SPA 需真 MouseEvent(已实测);② **深页(中间页折叠在 `...` jumper 后)点 NEXT 线性翻到 30 页 + 深处快速连点是否触发 captcha/拒签未实测——番茄 search 实发 `totalPages=30`、UI 会放用户翻到 30,故这是 shipped 路径的 gating 验收项(tasks 4.3),非纯残留**;③ headless 被 secsdk 拒签致空 → 沿用 `render-fetcher` 失败降级;④ 每翻一页 N-1 次点击 O(N) 成本 + **`h` 回翻对称重渲染**(无状态,无反向优化)+ **BROWSER_LOCK 全程持有点击循环**(串行化其它 render/solve/login)——search 现实 N≤5、池热,可接受;⑤ CDP 通道 back-pressure(实测 1-2min 停顿)→ 超时要宽、勿把拥塞误判成到头。
