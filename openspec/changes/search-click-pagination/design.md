## Context

`render-list-pagination` 给 explore 开了渲染取页,`render-dual-source` 让 search 能显示「第 N / M 页」(从分页器 DOM 读 `totalPages`)。但 search 主请求 URL 模板 `{{base}}/search/{{key}}` **不带 `{{page}}`**——UI 递增 `page` 重调 `search(key, N)` 导航同一 URL,SPA 初始请求恒 `page_index=0`,**每页都回重复的第 1 页**,而 `totalPages=30` 还 advertise 了翻不到的 29 页。

真浏览器 spike(agent-browser 实测 + 独立复核,2026-06-11,「十日终焉」300 项 / 30 页)的关键证据,保留备查、指导实现(查询串 **decoded 便于阅读**,实捕为 urlenc,如 `filter=127%2C127%2C127%2C127`):

```
番茄 search 翻页 —— 点击驱动(URL 不认页码)
  地址栏恒 /search/{urlenc 词} 不变;分页态只在 React 内存 + 签名 API
  每页 = 一次 GET /api/author/search/search_book/v1
        ?page_index=(人类页-1)&page_count=10&query_type=0&filter=127,127,127,127
        &query_word={词}&msToken=<每请求变>&a_bogus=<每请求变>
  分页器 = 字节 byte-pagination 组件:
    <ul.byte-pagination-list>
      <li.byte-pagination-item-icon[.disabled]>  PREV  svg.byte-icon-left   (第1页 disabled)
      <li.byte-pagination-item-active[data-active=true]>1</li>
      <li.byte-pagination-item>2..5</li>
      <li.byte-pagination-item-jumper>  "..." 中间页折叠
      <li.byte-pagination-item>30</li>           末数字项 = 总页数
      <li.byte-pagination-item-icon[.disabled]>  NEXT  svg.byte-icon-right  (末页 +disabled)
```

**对抗性反驳已做且失败**:6 种直达 URL 变体(`/page_2`=404、`?page_index=1`、`?page=2`、`?page_index=1&page_count=10`、`#/page/2`、`?p=2&offset=10`)无一到第 2 页,5 个静默回第 1 页。**by:url 对番茄 search 不可行,点击是必须。** `a_bogus`/`msToken` 每请求由页内 secsdk 重签、绑上下文不可复刻(`render-fetcher` 铁律)。约束:纯 serde 配置、规则即数据;向后兼容(翻页行为逐字节);渲染仅 `browser` feature、失败优雅降级;`search` 须保 `Send`(无 async 闭包)。

## Goals / Non-Goals

**Goals:**
- 让 render 拦截型 search 在「URL 不认页码」时真正翻页:书源声明「下一页」选择器,引擎点击驱动取第 `page` 页。
- 把 spike 实测的健壮性(稳健点击投递 / `page_index` 相关性 / 软封锁 reload-once / page-N DOM 落定)固化进实现,使深页与冷启动可用。
- 软封锁恢复一并修复第 1 页偶发误报。
- `pageBy` 缺席时翻页行为零变化。

**Non-Goals:**
- **不改 UI 翻页模型**:仍「单页 + 用户递增 `page`」,本 change 不动 `find_book`。**注意 fanqie search 到头停翻是单信号**——`fanqie-web.v2.json` 的 `search.request` 只配 `totalPages`、**无 `hasMore`**(`hasMore` 仅 explore 的 `book_list/v0` 有),故 `find_book.rs` 的 `at_end` 实际只靠 `page >= total_pages` 兜底(`has_more` 为 `None`)。这要求 page-N DOM 的 `totalPages` 求值稳定(见 D4 ④);若它失稳为 `None`,用户可一直按 `l` 越过第 30 页、每次触发整段 render+点击循环(经超时停)。是否给 search 补第二信号(如从 `search_book/v1` body 取 `has_more`/总数)留 Open Questions。
- **不做有状态跨调用浏览会话**(Opt 2):活页只在一次取页内存活(见 D1)。
- **不给 explore 接 `pageBy`**:番茄 explore `/library/all/page_{{page}}` 是 URL 驱动(by:url 已可翻)。
- **不破解/不复刻签名**:每页请求由站点 JS 自签(D7)。
- **不做深页跳转优化**(jumper / 页码输入直达):点击循环只用 NEXT 线性翻;深页若撞问题留后续(Open Questions)。

## Decisions

**D1 — Opt 1(无状态重点击,引擎内)胜过 Opt 2(有状态会话)。**
- by:click 本质是**顺序游标**(page N 只能从活着的第 N-1 页点出来),和 UI 的**无状态随机访问** `search(key, page)` 冲突。Opt 1:一次 `search(key, N)` 在**一张活页**(现有 `with_pool_page` 闭包)里点 NEXT `N-1` 次、返回**正好第 N 页**——零 UI 改、零跨调用会话状态。
- 备选(否决)**Opt 2**(活页跨 UI 翻页常驻,O(1)/次):省点击但要背会话生命周期 / 失效 / 单页槽争用,且 Opt 1 的全部摩擦它一个不少。Opt 1 的 O(N²) 累积点击对 search 现实 `N≤5` 无意义,浏览器池热。
- 备选(否决)**by:url**:spike 6 变体全失败,番茄 search URL 不认页码。
- 不重蹈 `render-list-pagination` 回退的 by:url 覆辙:那次回退是「引擎一次 batch 累积 N 页、连开 N 浏览器、内容重复」;Opt 1 是**单页返回不累积**,一次只开/复用一张活页——**完全满足 render-list-pagination「引擎 MUST NOT 自行批量翻多页」的规范约束**(该 MUST NOT 针对累积 + 连开多浏览器)。

**D2 — `pageBy` 是书源规则(规则即数据),只补 click 一种;是 render-list-pagination 单页机制的 additive 扩展。**
- 不复活完整 `nextPage` enum:`by:url` 作为*配置*冗余——URL 模板 `{{page}}` 本身就是 url 驱动机制(explore 已用)。本 change 只缺 click(无法用 URL 模板表达)。
- **语义定位**:render-list-pagination 的「单页取页」此前只有一种机制(`{{page}}` 映射 URL → 一次取一页);本 change **新增**第二种单页机制(点击到第 N 页),**单页语义(只返回第 N 页、只开一个浏览器、不累积)不变**。故为 additive、不构成对其需求的 MODIFIED(Modified Capabilities = 空,理由是语义而非"未归档"技术性)。
- 配置极简、内联挂 `Request`:`pageBy: { click: "<next 选择器>" }`。用 `{ click: ... }` 而非裸 `nextClickSelector: String`,留 `by` 扩展位且语义自描述;**当前只实现 click 一支**。贴 `render`/`readyFor`/`interceptApi` 先例内联(不 flatten,`deny_unknown_fields`,Option+`serde(default)` 不破向后兼容),camelCase、`skip_serializing_if` none。
- 仅在 `intercept_api` 存在 + `pageBy` 存在 + `page>1` 时启用点击循环;否则现状 `render_intercept`。

**D3 — 番茄 search 分页器选择器(spike 实测,逐字节)。**
- NEXT = `.byte-pagination .byte-pagination-item-icon:has(.byte-icon-right)`(永远 list 末 `<li>`、翻页不漂移);PREV = `:has(.byte-icon-left)`。prev/next **无 aria/text**,唯一判别是子 svg `byte-icon-right` vs `byte-icon-left`(`:has(svg.byte-icon-right)` 最稳)。

**D4 — 稳健点击投递 + 前进断言 + page_index 相关性 + DOM 落定(spike 实测必须)。**
- **点击投递**:NEXT li 渲染在折叠下方(实测 y≈2534),agent-browser CLI 单击返回成功但页未翻;**派发真 `MouseEvent` 才稳翻**。本仓用 chromiumoxide:点击前 `scrollIntoView`、派发真实指针事件(`element.click()` 或 evaluate 派发 `MouseEvent`,必要时 CDP `Input.dispatchMouseEvent` 按 box 坐标)。
- **前进的通用真信号 = 等「点击之后到达的、URL 含 `intercept_api` 的新响应」**:不对 `byte-pagination` 的 `.byte-pagination-item-active` DOM 硬依赖(保通用)。
- **`page_index` 相关性(强制,非可选)**:D5 的 reload-once 会向同一请求流注入残留的 `page_index=0` 重取响应,**纯端点 substring 匹配会误采**。故原语 MUST:①只认 click *之后*到达的响应(忽略点击前已见);②当响应 URL 携带页码参数时,按**期望 `page_index = page-1`(从响应 URL query 解析,非从 config)** 对齐,丢弃不匹配者。`intercept_api`(`search_book/v1`)本身不带 page_index,期望值由引擎从目标页算出。
- **`intercept_body` 须拆**:现 `intercept_body` 自挂监听 + 自 `page.goto`,**无法原样复用**于「点击后在已加载页等下一个响应」。须拆成「arm 监听一次 + 首页 goto」与「等下一个匹配响应(按上述相关性)」两段,`EventResponseReceived` 的 `EventStream` **跨整个点击循环持有**(chromiumoxide 0.9 的 stream 持续 yield,可行)。
- **不假设「一次点击=翻一页」**:点击后期望响应未到 → 在该页超时内重试点击 / 滚动入视重派 / 终止。**消歧**:用 NEXT 的 `disabled`/缺失(`click_next` 返回 `AtEnd`)区分「真到头」与「点击失败/拥塞(控件可点但没翻)」——**真到头**返回当前(末)页;**点击失败**重试一次(`CLICK_RETRY`),**重试耗尽仍未达目标页 → 报错传播**(上层 `RENDER_RETRY` 整页重试 / 优雅降级),**不返回更早的页冒充第 N 页**(CDP back-pressure 下慢响应不能被当成功)。
- **空 body 信号(实现约定,评审纠正)**:`wait_matching_body` 对「匹配到响应但 body 空」返回 `Ok("")`(非 `Err`),`Err` 只留给「完全没匹配到响应」。否则 `response_body` 把空 body 映射成 `None`、函数返回 `Err`、`.await?` 短路,D5 的 reload-once 守卫会成死代码。`Ok("")` = 软封锁精确信号,触发 reload。
- **page-N DOM 落定(④)**:点击后 **DOM 异步于网络响应更新**(实测新 `search_book/v1` ~1s 后发、DOM 随后更新)。原语在抓第 N 页 `outerHTML`(供 `render-dual-source` 的 `via:css` totalPages)前 **MUST 等 DOM 落定**——poll `.byte-pagination-item-active` 文本 == 目标页,或对 `dom_ready`/`readyFor` 重等一次——否则抓到 stale/半截分页器,`totalPages` 读错/为 `None`(直接打掉上面单信号停翻)。这是与第 1 页路径(首帧分页器即在)的关键差异。

**D5 — 软封锁 reload-once 恢复(触达 sibling 首页路径,严格改善)。**
- spike 实测:冷启动 / stale `a_bogus` → `search_book/v1` 回 **HTTP 200 空 body** + `.muye-search-empty`「共 0 项」+ `verify.zijieapi.com` 滑块 iframe;**reload 一次即恢复**(300 项 / 30 页)。
- 通用最小化:**拦到空 body → reload 当前页一次重试**(无害通用兜底);empty-state / captcha 选择器检出作可选增强信号。reload 后仍空才判零结果 / 失败。
- **此恢复同样接入现有 `render_intercept`/`intercept_page` 首页路径**(不止翻页),顺带修现状偶发「0 结果 / 未拦截到」误报。**诚实标注**:这**改变了所有 render+intercept 源第 1 页的行为**(此前空 body 直接作失败,现在 reload 一次再判),非纯 additive;但它**严格改善失败态**(空 body 本就是失败、reload 绝不改变原本成功的结果),只多一次 reload 延迟。故 change 级「`pageBy` 缺席 = 零行为变化」收窄为「**翻页行为**零变化」。

**D6 — 到头停翻信号(结构快路为主、超时为正确性兜底)。**
- 末页 NEXT li 加 `disabled` **class**(无 aria-disabled / disabled 属性),点了不发新请求。spike 直接观测到 p30 的 disabled NEXT;独立复核未走到 p30,仅经 PREV 的 disabled 切换(p1→p2)结构性佐证——故该结构信号**直接证据 + 结构佐证**,确定性不拔满,但**超时兜底使其错了也非致命**。
- 信号取舍:**超时 SHALL 为正确性保证**(对任意 SPA 成立);**控件暴露禁用态时,引擎 SHOULD 用 `:not(.disabled)` 守卫作主快路**(即时,免每个末页空等一次超时),并兼作 D4 的点击失败/拥塞消歧。
- UI 到头停翻不变(对 fanqie search 实为 `total_pages` 单信号,见 Non-Goals)。

**D7 — 不破解 / 不复刻签名(沿用 `render-fetcher` 铁律)。** `a_bogus`/`msToken` 每请求由页内 secsdk 重签、绑浏览器上下文不可外带,必须驱动真 SPA 自己点、自己签。boa 定位仍是确定性纯计算逃生舱(md5/aes/base64),非浏览器模拟器。

## Risks / Trade-offs

- [点击投递在 secsdk 重型 SPA 不稳(实测 CLI 单击失效)] → `scrollIntoView` + 派发真实 `MouseEvent` + 等点击后期望 `page_index` 响应判前进 + `:not(.disabled)` 消歧 + 失败重试/超时终止(D4)。
- [深页:中间页折叠在 `...` jumper 后,点 NEXT 线性翻到第 30 页 + 深处快速连点是否触发 captcha/拒签未实测] → 番茄 search 实发 `totalPages=30`、UI 放用户翻到 30,**这是 shipped 路径**;点击循环只用 NEXT(不依赖中间数字按钮),理论可线性翻,但 **tasks 4.3 列为 gating 验收**(失败则限深 / 降级),非纯残留。
- [**`h` 回翻对称重成本**] → 无状态 Opt 1 下,page N-1 同样重 render + 重点击 N-2 次(无反向点击优化,PREV 选择器虽实测但不用);每次回翻按键都是一次完整 render。N≤5 可接受,但实现者须知这是 Opt 1 固有(tasks 4.3 含 `l,l,h` 回翻观察)。**已交付缓解**:`Engine.page_cache`(渲染结果按 `操作+词/分类+页+页大小` 缓存,per-source 会话级、仅 render 路径)使**回翻/重访已取页即时**(命中缓存、不再驱动浏览器),只首访某页付点击成本——把「来回翻」从每次 O(N) 降到只在新页付费,而无需 Opt 2 的有状态会话。
- [**BROWSER_LOCK 全程持有整段点击循环**] → `with_pool_page` 整个闭包持全局 `BROWSER_LOCK` + 单 `RENDER_POOL` 槽;一次 `search(key,N)` 现持锁 ≈ `N` 个 render 的时长(每页 click + ~1s 响应 + 可能 reload + 末页超时兜底),期间**所有其它书源 render 及同 profile 的 solve/login 全被阻塞**。设计只分析点击数、未分析锁占用时长。bound = 每页超时 × `target_page`(最坏);N≤5 可接受,深页/软封锁时可达数十秒——tasks 4.3 验其它 op 不被饿死,必要时限 `target_page`。
- [快速连点是否重触发软封锁 / captcha 未知] → 点击间留就绪等待(等期望响应即天然节流);撞空 body/empty-state/captcha → reload-once(D5);反复失败优雅降级。
- [**CDP 通道 back-pressure**] → 独立复核实测 agent-browser 守护进程 `Resource temporarily unavailable (os error 35)`、命令停顿 1-2min。驱动同站的 `render_intercept_paged` 的**每页响应超时须足够宽**,且**勿把"慢但在途(拥塞)"误判成到头(D6)或软封锁(D5)**;用 `:not(.disabled)` 在场判断真到头(无请求发出),而非单凭超时。
- [headless 被 secsdk 拒签致空] → 沿用 `render-fetcher` 失败降级(`RENDER_FAILED` 会话熔断)。
- [O(N) 点击/次、O(N²) 累积] → search 现实 `N≤5`、池热,可接受;海量浏览走 explore by:url(99 页)。
- [`Page.captureScreenshot` 在该 secsdk SPA 挂(spike 发现,截图全超时)] → 仅影响 QA 截图,不影响功能;实跑靠 DOM-state + 网络 `page_index` 断言。
- [`search` 引入点击循环可能诱发 async 闭包破坏 `Send`] → 原语在 `fetcher.rs` 内同步顺序 await,`search` 仍普通 `async fn`;主程序 `cargo build` 验证 `Send` 不回归。

## Migration Plan

- 纯增量:`search.request` 新增可选 `pageBy`,旧书源不动、翻页行为不变;`pageBy` 缺席 = 现状单页(`render_intercept`)。唯一行为改动是软封锁 reload-once 触达所有 render+intercept 首页(D5,严格改善)。
- 落地验证:`fanqie-web.v2.json` `search.request` 加 `pageBy.click`;UI 实跑——番茄搜索翻 2-3 页内容**不重复**、翻到第 30 页后「下一页」停翻;`l,l,h` 回翻可用;首页偶发软封锁经 reload 恢复;深页(接近 30)线性翻稳定 + 其它 op 不被锁饿死(gating)。
- 回滚:移除 `search.pageBy` 即回现状(每页重复第 1 页的旧表现复现,但无数据迁移);移除 `render_intercept_paged` 调用即回 `render_intercept`(reload-once 若想一并回退,从 `intercept_page` 摘除)。

## Open Questions

- **search 第二停翻信号**:`fanqie-web.v2.json` search 现仅 `total_pages` 单信号(无 `has_more`)。是否从 `search_book/v1` body 取 `has_more` / 总数(需 spike 确认该 body 有此字段)作第二信号,加固「DOM totalPages 失稳」时的边界。暂留。
- **深页线性翻 vs 跳转**:点 NEXT 连翻到第 30 页是否稳(中间不踩软封锁 / 不丢点击 / 不被 anti-bot 拒)?若深页不稳,是否引入 jumper(`.byte-pagination-item-jumper`)/ 页码输入直达——但那又是 byte-pagination 特定 DOM,与「通用点击翻页」张力。tasks 4.3 实跑定。
- **`pageBy` 是否给 explore 留**:番茄 explore 用 by:url 不需要;`pageBy` enum 口子可让「URL 不变的 SPA explore 书源」复用,暂不接、不实现。
- **总页数 DOM 取自哪一页**:click-pagination 驱动到第 N 页时,`render-dual-source` 的 DOM 源 `totalPages` 取自**落定后的第 N 页 DOM**(byte-pagination 任一页都暴露末数字项 = 30,组合稳定);D4 ④的 DOM 落定保证它非 stale。两 capability 据此可组合,无需重读第 1 页。
