## 0. 前置 spike(真浏览器)—— 已完成(agent-browser 实测 + 独立复核 2026-06-11)

- [x] 0.1 番茄 search 翻页 = **点击驱动**:地址栏恒 `/search/{词}` 不变,每页一次签名 GET `/api/author/search/search_book/v1?page_index=(页-1)&page_count=10&query_word=…&a_bogus=…&msToken=…`(`a_bogus`+`msToken` 每请求重签)。点 NEXT → `page_index` 0→1、URL 不变、active 页 1→2(已复核)
- [x] 0.2 **对抗性反驳失败**:6 种直达 URL 变体(`/page_2`=404、`?page_index=1`、`?page=2`、`?page_index=1&page_count=10`、`#/page/2`、`?p=2&offset=10`)无一到第 2 页 → by:url 不可行,点击是必须
- [x] 0.3 分页器选择器(逐字节):NEXT=`.byte-pagination .byte-pagination-item-icon:has(.byte-icon-right)`(永远 list 末 li、不漂移);PREV=`:has(.byte-icon-left)`;prev/next 无 aria/text,唯一判别是子 svg `byte-icon-right`/`left`;active 页=`.byte-pagination-item-active[data-active=true]`。**末页 NEXT 加 `disabled` class**(无 aria/attr)、点了不发请求——spike 直接观测到 p30、独立复核经 PREV(p1 禁用→p2 启用)结构性佐证,**直接证据 + 结构佐证**(确定性不拔满,但 D6 超时兜底使其错了也非致命)
- [x] 0.4 四摩擦实测:① 点击投递不稳(NEXT 在折叠下方 y≈2534,CLI 单击失效,真 MouseEvent 才翻)② 软封锁(冷启/stale a_bogus → 200 空 body + `.muye-search-empty` + verifycenter 滑块,reload 一次恢复)③ 响应相关性(避开 reload 残留 page_index=0,须按目标 page_index 对齐)④ 点击后 **DOM 异步更新**(新请求 ~1s 后发、DOM 随后,须 poll active 落定再抓 DOM)+ CDP 通道 back-pressure(os error 35、停顿 1-2min,超时须宽)

## 1. 配置类型与 schema

- [x] 1.1 `source/http.rs`:`Request` 内联新增 `pageBy: Option<PageBy>`(`#[serde(default, skip_serializing_if="Option::is_none")]`,camelCase `pageBy`);定义 `PageBy { click: String }`,**与 `Retry`/`RateLimit` 等同款** `#[serde(rename_all="camelCase", deny_unknown_fields)]` + `#[cfg_attr(feature="schema", derive(schemars::JsonSchema))]`(留 `by` 扩展位但当前只 click 一支),贴 `render`/`readyFor`/`interceptApi` 先例(Option+default 不破 `deny_unknown_fields` 向后兼容)
- [x] 1.2 `fetch/mod.rs`:`FetchRequest` 承载点击翻页所需(`page_by: Option<String>` 的 next 选择器 + 目标页 `page: u32`);文档界定仅 render+intercept 路径用、默认无;非 `..Default` 构造处补默认。**注:`page` 字段须在 3.1 的 `req.page>1` 路由判定前存在(1.2 是 3.1 的前置)**
- [x] 1.3 schema 重生成(`cargo run -p parse-book-source --features schema --example gen_schema > crates/parse-book-source/book-source.schema.json`)+ `schema_is_in_sync` 绿。**该测试是强制 gate——不重生成即 FAIL,非可选**

## 2. 浏览器原语 render_intercept_paged

- [x] 2.1 `fetch/browser/fetcher.rs`:新增 `render_intercept_paged(url, api_contains, target_page, next_selector, timeout, headless, dom_ready) -> Result<(String, Option<String>)>`,在 `with_pool_page` 闭包内:① goto + **arm 监听一次**、等首页(`page_index=0`)响应 ② `for p in 2..=target_page` { 稳健点击 NEXT(2.2)+ 等点击后、按 `page_index=p-1` 对齐的新响应 } ③ 抓末页 body(+ 可选 DOM,见 ④)。**`intercept_body` 须拆**:它现自挂监听 + 自 `goto`,无法原样复用于「已加载页等下一个响应」——拆成「arm(`EnableParams` + `event_listener` + 首页 goto)」与「等下一个匹配响应(URL 含 `api_contains` 且 page_index 对齐、忽略点击前已见)」两段,`EventResponseReceived` 的 `EventStream` **跨整个点击循环持有**(chromiumoxide 0.9 stream 持续 yield)
- [x] 2.2 **稳健点击投递 + 前进断言**(D4):抽助手——`scrollIntoView`(NEXT 选择器)+ 派发真实 `MouseEvent`(`element.click()` / evaluate 派发,必要时 CDP `Input.dispatchMouseEvent` 按 box 坐标);**前进判据 = 等「点击之后到达、URL 含 `api_contains`、按目标 `page_index` 对齐」的新响应**(不依赖 `.byte-pagination-item-active` DOM)。`page_index` 对齐为**强制**(排除 reload 残留 page_index=0)。MUST NOT 假设一次点击=翻一页:未前进且控件可点 → 重试(滚动重派)/ 超时终止
- [x] 2.3 **到头停翻**(D6):**超时 = 正确性保证**(点击后超时内无对齐新响应 → 到头);控件暴露禁用态时 **SHOULD 以 `:not(.disabled)` 守卫作主快路**即时判到头,并据此**消歧**「真到头(控件禁用)」vs「点击失败/拥塞(可点但未前进)」——后者重试、不当到头。**CDP back-pressure**:每页响应超时取足够宽,勿把慢但在途误判成到头/软封锁
- [x] 2.4 **软封锁 reload-once**(D5):抽助手——拦到**空 body**(或检出 `.muye-search-empty`/captcha)→ reload 当前页一次重试(reload 后仍空才判空);接入 `render_intercept_paged` **和** 现有 `render_intercept`/`intercept_page`。**诚实标注**:这改变所有 render+intercept **首页**行为(空 body 此前直接失败,现 reload 一次再判)——严格改善失败态、**不改原本非空成功结果**;在原语/escalating 文档与知识库写清
- [x] 2.5 **page-N DOM 落定**(D4 ④):抓第 N 页 `outerHTML`(供 `via:css` totalPages)前 **MUST 等 DOM 落定**——poll `.byte-pagination-item-active` 文本 == 目标页 或对 `dom_ready`/`readyFor` 重等一次(点击后 DOM 异步于网络响应,直接抓会得 stale 分页器、`totalPages` 读错/None);第 1 页路径无此问题(首帧分页器即在)
- [x] 2.6 公开 API 文档(`-D warnings`):新原语 + 拆分后助手的 rustdoc;私有项 intra-doc 链接用纯 code span(避免 doc private 链接报错)

## 3. 引擎 / 装饰器接线

- [x] 3.1 `fetch/browser/escalating.rs` render 分支:`intercept_api` 存在 + `req.page_by` 存在 + `req.page > 1` → 走 `render_intercept_paged`(传 `page_by`/`page`/`ready_for` 作 `dom_ready`);否则现状 `render_intercept`。**前置依赖 1.2**(`FetchRequest.page` 须先存在,谓词才编译)。**注**:外层 `RENDER_RETRY` 会**重跑整段 paged 原语**(重开页、重点 N-1 次)于任何非 `Browser` Err——N≤5 可接受,文档注明,勿让外层重试与内层 reload-once 复合成多次整页重导航
- [x] 3.2 `engine/mod.rs` + `internal.rs`:`search` 把 `op.request.page_by` + `page` 透传进 `send_templated_full`/`FetchRequest`(新增参数或扩展现有签名);`explore` **不变**(URL 驱动)。`search` 保持普通 `async fn`、无 async 闭包(`Send` 不回归)
- [x] 3.3 引擎测试(假 fetcher,贴 `RenderProbe` 先例):点击翻页路由——`page=1` 不触发 page_by、`page=3` + pageBy 走 paged 路径(用替身验证「请求里带了 page_by + page」即可,真点击/相关性在 UI 实跑验证);`pageBy` 缺席走单拦截

## 4. 番茄接入与验证

- [x] 4.1 `fanqie-web.v2.json`:`search.request` 加 `pageBy: { click: ".byte-pagination .byte-pagination-item-icon:has(.byte-icon-right)" }`;explore **不动**(by:url);`from_json` 解析验证
- [x] 4.2 `cargo test --all-features` 全绿 + `clippy -D warnings` + `doc -D warnings` + `fmt`;`cargo build` 主程序验证 `search` Future 仍 `Send`(子 crate lib test 测不出 spawn 上下文)
- [ ] 4.3 UI 实跑(**待用户浏览器环境**)—— 含 **gating 验收项**:
  - 番茄搜索翻 2-3 页内容**不重复**、`l,l,h` **回翻**可用(观察回翻重渲染延迟,确认 Opt 1 对称成本可接受)
  - 翻到第 30 页后「下一页」停翻;首页若撞软封锁经 reload 恢复
  - **(gating)深页**:线性翻到接近 30 页是否稳(中途不踩软封锁 / 不丢点击 / 不被 anti-bot 拒)——失败则限深 / 降级
  - **(gating)锁占用**:一次多页 search 进行时,**其它书源 render / solve-login 不被 `BROWSER_LOCK` 饿死**(深页/软封锁可达数十秒)

## 5. 文档

- [x] 5.1 知识库 `dev-notes/knowledge/booksource.md`:番茄 search **点击驱动翻页**(pageBy.click + 选择器 + 末页 disabled 信号)+ **四摩擦**(点击投递真 MouseEvent / 软封锁 reload-once 触达首页 / **page_index 强制相关性避 reload 残留** / **page-N DOM 落定** + CDP back-pressure)+ **BROWSER_LOCK 全程持锁串行化**注意 + by:url 反驳结论
- [x] 5.2 `booksource-generator` skill 文档(如适用):click 驱动翻页配置 + 选择器/软封锁约定
