# 书源 / parse-book-source / TTS

## 概览

`crates/parse-book-source`（Legado 风格书源解析引擎、规则 DSL、反爬/渲染抓取）与 `crates/novel-tts`（Kokoro TTS）的项目特有约束。重点是番茄（fanqienovel.com）这类 SPA + 签名站点的接入路线。

## 规则引擎

### 抓取分层 fetch / render

`fetch_checked` 是纯 reqwest（拿静态 HTML / 普通 API），**SPA 站点拿不到数据**（首屏是空壳，数据靠 JS 拉）。SPA 走 **render 通道**（CDP 驱动真浏览器渲染 + `interceptApi` 拦签名 API 响应），由 `fetch/browser/` 下的 fetcher + EscalatingFetcher 实现。

**render 三字段在 `Request` 上**：`render`（开渲染）/`readyFor`（等待选择器/条件）/`interceptApi`（拦哪个 API 的响应当数据源），连同 `totalPages`/`hasMore`/`pageBy`/`vars` 全部收敛在 `source::http::Request` 上。search 与 explore 因此共用同一份「列表页规格」`ListPageSpec`（见下「explore 两阶段 + 共享 ListPageSpec」），不再各自抄一份渲染字段。

**相关文件**：`crates/parse-book-source/src/fetch/browser/`、`src/source/op.rs`

### 常驻浏览器池（browser-pool）：render 复用浏览器、只开新 Page

render 路径（`render_dom`/`render_intercept`）**复用同一常驻 `Browser`、每次只开新 `Page`**，用完 `page.close()` 留 `Browser`，免去每次 launch 的秒级开销（翻页顺滑；也是 P2 点击翻页的基础）。`solve`/`login`（headful）**不**走池，仍每次 `launch_ephemeral` 临时起、用完 close。两类 headful/render 生命周期分别由 `with_ephemeral`/`with_pool_page` 收口。

几个非显然点：

- **池必须是进程级单例**（`static RENDER_POOL: Mutex<Option<Resident>>`），**不能**做成 `BrowserFetcher` 的实例字段。因为所有书源共享同一持久 profile（`~/.novel/browser-profile`）：若每个 `BrowserFetcher`（每次 `build_engine` 新建一个）各持一个常驻浏览器，多书源/路由回退栈场景下两个实例会同时存活、互抢 profile 的 `SingletonLock`（后建者 `spawn_browser` 无条件删锁）→ profile 数据竞争/「配置文件已在使用」。`BROWSER_LOCK` 只保证「同一时刻只一个浏览器在启动/渲染」，**常驻化把「存活」与「持锁」解耦后，「跨实例存活并存」不再受锁保护** —— 全局单例才能根治。
- **关 Page 不会让浏览器退出**：Chrome 经 CDP 调试连接启动后，只要调试连接（handler task）在连就存活，零标签页也不退 —— 所以「关 render 的 Page、留 Browser」成立，池化真实生效。浏览器退出只发生在 `Browser::close()`/进程崩溃。
- **headful 解挑战/登录前必须先拆常驻渲染浏览器**：`launch_ephemeral` 里先 `shutdown_render_pool().await`（优雅 `close` 释放 profile 的 `SingletonLock`）再起 headful 实例。
- **`handler.is_finished()` 不是可靠断连探活**：常驻浏览器从不 `close()`，崩溃/断连时 handler task 多半 parked 在 `Pending`、`is_finished()` 仍为 false（只有显式 `Browser::close()` 才结束）。它只是廉价乐观快路；**真正的断连兜底是 `new_pool_page` 开页失败/超时后拆掉重建**。且死浏览器的 `new_page` 不会立刻报错，要等 CDP 默认 30s 请求超时，期间还独占 `BROWSER_LOCK` 阻塞所有书源 —— 故给开页套一道短超时（8s），超时即判断连重建。
- **锁序**：render 整段持 `BROWSER_LOCK`，池的取/建/拆都在其下（`BROWSER_LOCK → RENDER_POOL`），与 `launch_ephemeral` 同序，无死锁。
- **退出收尾走显式 `shutdown_render_pool()`，不能靠 `Drop`**：池是 static，进程退出**不触发** `Drop`；且即便给 `BrowserFetcher` 加 `Drop` 也错（任一 engine drop 会杀掉别 engine 仍在用的全局浏览器）。故 app 在退出点（`src/lib.rs` 的 `App.fullscreen().await` 之后）显式 `parse_book_source::shutdown_render_pool().await`，否则 headless 子进程被孤儿化。

**不要**：让 render 复用同一个 `Page`（SPA 状态/cookie 跨页串味，design 已否决）—— `new_page` 比 `launch` 快几个数量级，每次新 Page 足够。

**相关文件**：`crates/parse-book-source/src/fetch/browser/fetcher.rs`（`RENDER_POOL`/`new_pool_page`/`with_pool_page`/`with_ephemeral`/`shutdown_render_pool`）、`src/lib.rs`（退出钩子）、`openspec/changes/browser-pool/`

### render 双源（render-dual-source）：拦 API 的同时也能读渲染 DOM

render 的 `interceptApi` 会话**本来就在渲染一张真实页面**，拦 API（数据）之外可顺手抓渲染后 DOM（`outerHTML`），让 `via:css`/`xpath` 规则对 DOM 求值——典型用途：分页器的**精确总页数**（番茄 API 的 `total_count` 是占位 10000 不可靠，真数只在 DOM 分页器）。`explore`/`search` 返回类型因此从 `Vec<BookListItem>` 改为 `BookList { items, total_pages }`，UI 显示「第 N / M 页」。

- **信号 = `interceptApi` + `ready_for` 共存**：二者过去是「二选一」，现放宽为可同时给——`interceptApi` 取 body，`ready_for` 作 DOM 就绪闸（分页器在 API 之后才渲染，得等它出现再抓）。只配 `interceptApi`（无 `ready_for`）则不抓 DOM、`dom_html=None`，逐字节同现状。
- **路由 = 按规则 `via`（`Rule::primary_via` + `pick_source`）**：`via:css`/`xpath` 的规则打渲染 DOM（没抓到则退 body），其余（json/regex/raw）打 body。`Rule` 是枚举、组合规则（`firstOf`/`concat`）取首个子规则的主 via。**关键**：这让 `has_more`（番茄 `via:json` → API body）与 `total_pages`（`via:css` → DOM）**同会话共存正确**——别用「抓到 DOM 就一律打 DOM」的 dom-presence 路由，那会把 has_more 也错误地丢给 DOM（json 解析 HTML 失败)。
- **传输**：`FetchResponse.dom_html: Option<String>`，仅 render+intercept+要 DOM 时有值；`run_request_full`/`send_templated_full` 透传，普通取页仍用只回 body 的 `run_request`/`send_templated`。
- **番茄分页器选择器**（agent-browser 实测，explore 书库 + search 同一字节 `byte-pagination` 组件）：总页数 = `.byte-pagination-item:not(.byte-pagination-item-icon):not(.byte-pagination-item-jumper)` 取 `index:-1`（末数字项；过滤掉前后箭头 icon 与 `...` jumper；少页/单页也成立）。`parse_total_pages` 再从结果抽首段数字。

**相关文件**：`crates/parse-book-source/src/fetch/{mod.rs,browser/{fetcher.rs,escalating.rs}}`、`src/engine/{mod.rs,internal.rs}`（`eval_total_pages`）、`src/model.rs`（`BookList`）、`fanqie-web.v2.json`、`openspec/changes/render-dual-source/`

## 番茄（fanqienovel.com）接入

### 签名不可破解，render 让浏览器自己签

番茄 API 带字节跳动 secsdk 签名：`a_bogus`（每请求级，依赖 DOM/BOM/Worker 运行时算）+ `msToken`（会话级）。**boa 跑不了**这段 JS（需要浏览器环境），别想着把签名算法搬进 boa/Rust 复刻——验证过，不可行。唯一可行路线是 **render**：驱动真浏览器，让它自己带签名发请求，我们只 `interceptApi` 拦响应 JSON。

**不要做**：试图逆向 `a_bogus`/`msToken` 算法在 Rust 侧复刻。

### 两套 API：网页端 vs App 端

- **网页端**（fanqienovel.com）：无签名负担的部分有限——只前 ~10 章完整,其余试读 / 需会员。
- **App 端**：全本免费但需签名 + AES 解密。
- 字体解密是两端通用能力。

### 番茄有 ≥3 套混淆字体，各需独立 fontMap（content / search / explore 不通用）

番茄不同页面/接口用**不同的**字体反爬字体,码点（PUA E3xx–E5xx）范围重叠但**映射不同**——拿 `search` 的 map 去解 explore 书库的书名会得到一串乱码（「时停起手」被解成「美停它从」）。故 `fontMaps` 里 `content`（阅读正文,class 见 reader）、`search`（搜索结果）、`explore`（书库 `book_list/v0` + DOM class `font-fKts9tCXDjS49UhH`,字体文件 `…/awesome-font/c/e26e946d8b2ccb7.woff2`）**各是一套**,书源每个 op 的 item 规则按来源挂对应 `clean:[{fontMap:"…"}]`。

- **生成新字体的 map**：`cargo run -- gen-fontmap "<woff2 URL 或路径>" --out /tmp/x.json`（字形位图相似度匹配,自动下 Noto 基准;纯 Rust 零 C 依赖)。字体 URL 从页面 `@font-face` 的 `src` 取（`document.styleSheets` 里 `r.type===5` 的规则）。低置信（<0.55）项会标注、个别字可能错（与现有 content/search map 同等质量）。
- **怎么定位用哪套**：渲染该页,`TreeWalker(SHOW_TEXT)` 扫含 PUA（0xE000–0xF8FF）的短文本节点,看 `parentElement.className` 的 `font-xxx`,再比对各 map 解码是否通顺。
- **稳定性假设**：这些字体哈希在番茄基础设施上**相对稳定**（content/search map 静态内联已长期可用),故 explore map 也静态内联;若哪天轮换导致解码变乱码,重跑 `gen-fontmap` 更新即可。

**相关文件**：`fanqie-web.v2.json`（`fontMaps.{content,search,explore}`）、`src/gen_fontmap.rs`、`dev-notes/blog/font-anti-scraping-and-fontmap.md`

### explore 是 URL 驱动，search 是点击驱动（已落地 `search-click-pagination`）

- **explore 书库** `/library/all/page_N`：URL 驱动——翻页 URL 变,直接导航该 URL 即渲染第 N 页。API `book_list/v0`（`page_index=N-1` 0 基）。**`by:url` 翻页可用**（`{{page}}` 进 URL 模板,不需任何点击配置）。
- **search** `/search/{词}`：SPA **不认 URL 页码**（agent-browser 实测 6 种直达变体 `/page_2`=404、`?page_index=1`、`?page=2`、`#/page/2` 等**全部回第 1 页**——对抗性反驳失败,by:url 对 search 不可行）,只能点分页器「下一页」触发 `page_index` 递增。API `search_book/v1`,`a_bogus`+`msToken` 每请求重签。

### 点击驱动翻页 `pageBy.click`（render 拦截源,URL 不认页码时）

书源在 `request` 上配 `pageBy: { click: "<next 选择器>" }`;`page > 1` 时引擎在**一张活页**内点 `page-1` 次翻到目标页、拦该页 API 响应。配置极简、只补 click 一种（by:url 用 `{{page}}` URL 模板已覆盖,不复活完整 enum）。`pageBy` 缺席 = 现状单拦截,翻页行为逐字节不变。

- **番茄 search 分页器选择器**（`byte-pagination` 组件,逐字节实测）：NEXT = `.byte-pagination .byte-pagination-item-icon:has(.byte-icon-right)`（永远 list 末 `<li>`、翻页不漂移）;PREV = `:has(.byte-icon-left)`。**prev/next 无 aria/text,唯一判别是子 svg `byte-icon-right` vs `byte-icon-left`**。末页 NEXT 加 `disabled` **class**（无 aria-disabled/disabled 属性）、点了不发请求。当前页 = `.byte-pagination-item-active[data-active=true]`。
- **四摩擦**（点击翻页 `intercept` 必须处理,否则一定踩）：
  1. **点击投递**：NEXT 渲染在折叠下方（实测 y≈2534）,CDP/CLI 单击「成功」但页没翻;**`scrollIntoView` + evaluate 派发真 `MouseEvent`(mousedown/mouseup/click)才稳翻**(触发站点 React)。
  2. **`page_index` 强制相关性**：前进判据 = 等「点击之后到达、URL 含 `interceptApi`、且 `page_index == 目标页-1`」的响应（`url_page_index` 解析）。**必须按 page_index 对齐**——软封锁 reload 会注入残留 `page_index=0`,纯 substring 匹配会误采。监听流（`EventResponseReceived`）**跨整个点击循环持有**,自然只认「点击后到达」的。
  3. **软封锁 reload-once**：冷启/stale 签名 → `search_book/v1` 回 **HTTP 200 空 body** + `.muye-search-empty`「共 0 项」+ `verify.zijieapi.com` 滑块 iframe;**拦到空 body → reload 当前页一次**即恢复。此恢复也接进现有单页 `intercept_body`（第 1 页偶发软封锁同样修;空 body 本是失败态,严格改善、不改成功结果）。
  4. **page-N DOM 落定**：点击后 DOM **异步于网络响应**更新（新请求 ~1s）,抓第 N 页 `outerHTML`（给 `via:css` 的 totalPages）前**重等就绪闸**（`wait_ready(readyFor)`),免抓到半截分页器。注:番茄 totalPages 选择器取末数字项「30」,各页恒定,故 staleness 对它影响小,但仍重等保险。
- **统一的 `intercept`(单页 + 点击翻页一个方法)**：`render_intercept(url, api, timeout, headless, dom_ready, paging: Option<(目标页, next选择器)>)` 一个公开入口——`paging=None` 单页、`Some` 点击翻页(escalating 据 `pageBy`+`page>1` 计算 `paging` 单次调用)。核心 `intercept` 内:arm 监听(`responseReceived`/`loadingFinished`,**先挂再 goto**)→ 首页取页 + reload-once → 可选点击循环 → 可选抓 DOM,**只写一遍**(早期 `intercept_body`/`intercept_page`/`intercept_paged` 三方法的 arm/reload/DOM 重复已合并)。等响应抽成泛型 `wait_matching_body<R,F>`(持两个事件流、按 api 子串 + 可选 page_index 对齐等下一个匹配 body;匹配到但空 body 返回 `Ok("")` 作软封锁信号、非 Err)。
- **到头停翻 vs 点击失败(消歧很关键)**：点 NEXT 返回 `missing`/`disabled`（控件缺失/禁用）= **真到头** → 停、返回当前(末)页（D6 结构快路;`page > 实际总页` 由此自然收尾）。点了但目标页响应超时未到 = **点击失败/拥塞(非到头)** → **重试一次**（`CLICK_RETRY`,spec SHALL）;重试耗尽仍未达目标页 → **报错传播**（交上层 `RENDER_RETRY` 整页重试 / 优雅降级），**绝不静默返回更早的页冒充第 N 页**（CDP back-pressure 下尤其要紧）。两者别混:返回 `Ok(当前页)` 只在真到头时发生。
- **软封锁的「空 body」信号**:`wait_matching_body` 对「匹配到响应但 body 空」返回 **`Ok("")`**(非 `Err`),`Err` 只留给「完全没匹配到响应」。否则 `response_body` 把空 body 映射成 `None` → 函数返回 `Err` → `.await?` 短路,reload-once 守卫成**死代码**(评审实测发现)。空串 = 软封锁精确信号,交调用方 reload。
- **`BROWSER_LOCK` 占用**：`with_pool_page` 整个闭包持全局 `BROWSER_LOCK`;一次多页 search 现持锁 ≈ N 个 render 时长,期间其它书源 render / solve-login 全被串行阻塞（深页/软封锁可达数十秒）。search 现实 N≤5 可接受;真实 env 验深页线性翻稳定性 + 其它 op 不被饿死（`search-click-pagination` tasks 4.3 gating）。
- **CDP back-pressure**:独立复核实测 secsdk SPA 上 CDP 通道会 `os error 35`、停顿 1-2min → 每页响应超时要宽,勿把「慢但在途」误判成到头/软封锁。
- **回翻/重访的成本与缓解(渲染结果缓存)**:点击翻页是**无状态重点击**——UI 翻到第 N 页 = 开新活页从第 1 页点 N-1 次,**上一页也一样**(`target_page=page-1`,从头少点几次,根本不点 PREV)。故回翻/重访已看过的页本会重付 O(N) 点击。`Engine` 加了 **`page_cache`**(`Arc<RwLock<HashMap<键, BookList>>>`,随 Clone 共享、per-source 会话级)缓解:键 = `操作\0词或分类模板\0页\0页大小`,**仅 render 路径缓存**(reqwest 便宜且缓存会跳过 cookie 回灌/命名捕获副作用),命中即返回、不再驱动浏览器。回翻/重访已取页**即时**,只首访某页付点击成本。注:缓存的是「页数据」,explore 用静态分类 URL 模板作键(`UrlOrRule::Str`;`Rule` 形不缓存)。

**相关文件**：`crates/parse-book-source/src/fetch/browser/fetcher.rs`（`render_intercept`/`intercept`/`wait_matching_body`/`click_next`/`url_page_index`）、`src/source/http.rs`（`PageBy`）、`src/fetch/mod.rs`（`FetchRequest.page/page_by`）、`src/fetch/browser/escalating.rs`（render 分支路由,据 `pageBy`+`page>1` 算 `paging`）、`src/engine/{mod.rs,internal.rs}`（`page_cache` 渲染结果缓存、`RenderArgs` 参数束）、`fanqie-web.v2.json`（search.request.pageBy）、`openspec/changes/search-click-pagination/`

### explore 两阶段 + 共享 ListPageSpec（dynamic-explore-entries）

explore 不再是「分类 URL + 内联列表字段」，而是两阶段：`entries` 生成可选择的入口，`page` 用选中入口的变量取一页书。入口身份 = **标题 + 变量**（`ExploreEntry { title, vars }`，运行时类型在 `model.rs`），不再是固定 URL——取页 URL 由 `explore.page.request.url` 用入口变量 + `{{page}}` 模板生成。

- **`ExploreOp { entries: Vec<EntrySource>, page: ListPageSpec }`**。`entries` 是入口源数组，按声明顺序合并（**没有独立 chain 类型**——「按序合并」就是数组遍历本身；包成枚举只会多一层嵌套）。
- **`EntrySource`**（`untagged` + 唯一键判别，同 `Rule`）：`static`（固定入口列表 `{title, vars: BTreeMap<String,String>}`）/ `fetch`（远端抓取，`Box` 包裹避免大变体）。`fetch` 含 `forEach`（多组变量重复请求并合并，空=一次）、`request`（**复用 `Request`**，白送 render/intercept/charset/headers）、`list` 抽项、`item: {title: Rule, vars: BTreeMap<String,Rule>}`。
- **`item` 规则的求值上下文**：ctx = 当前数据项（`via:json` 读其字段），vars = base + 当前 `forEach` 循环变量（`{{name}}` 引用循环变量）。JS 规则里 `result` 是数据项 **JSON 字符串**，需 `JSON.parse(result).field`（不是已解析对象）。
- **`SearchOp = ListPageSpec`**（`pub type` 别名，裸用不包 `page` 层，search JSON 形状不变）。`ListPageSpec { prelude, request, list, item }` 就是历史 `SearchOp` 的形状——前序 change 已把全部取页旋钮收敛到 `Request`，故 search/explore 共用一个 runner。
- **引擎收敛到 `run_list_page(spec, kind, extra_vars, page, page_size)`**（`engine/internal.rs`）：search 传 `{key}`、explore 传入口变量;渲染结果缓存键含 `kind`('s'/'e') + 有序 `extra_vars` + 页 + 页大小（不同入口因变量段不同不串缓存）。explore 收敛后**白捡** search 已有的「响应命名捕获 `request.vars`」能力。
- **`Engine::explore_entries().await -> Result<Vec<ExploreEntry>>`** 替代旧同步 `explore_categories()`。**部分成功**：某动态源失败时保留已成功入口（含静态源），不阻断；仅当零入口产出且有源报错才返回 Err。**仅完全成功才缓存**（`entries_cache`，per-source 会话级），失败的动态源下次进入可重试。
- **UI**（`select_books/mod.rs`）：入口加载本就在 `use_init_state(async move{})` 内，改 `explore_entries().await?` 即可；`ExploreListItem(ExploreEntry)`，取页把整个 entry 交给 `engine.explore(&entry, page, size)`。
- **番茄迁移**：静态入口「书库·最热/最新」（`vars: {filter:"all", sort:"hottest"|"newest"}`）+ page URL `{{base}}/library/{{filter}}/page_{{page}}?sort={{sort}}`，render/intercept/totalPages/hasMore/explore fontMap 全部挪到 `page.request`/`page.item`，对这两个入口字节等价。**动态分类入口**（按 gender forEach 调 `category_list/v0`）的活体正确性需 `trn doctor` 对站点验证;即便动态源失配，部分成功也会退化到静态入口、explore 仍可用。

**相关文件**：`crates/parse-book-source/src/source/op.rs`（`ListPageSpec`/`EntrySource`/`StaticEntry`/`FetchEntrySource`/`FetchEntryItem`/`ExploreOp`）、`src/model.rs`（`ExploreEntry`）、`src/engine/{mod.rs,internal.rs}`（`explore_entries`/`run_list_page`/`load_entry_source`/`load_fetch_entries`）、`fanqie-web.v2.json`、`openspec/changes/dynamic-explore-entries/`

### explore 单页 + UI 递增 page（不引擎批量翻页）

引擎的 `explore`/`search` 是**单页**纯 async fn（无 async closure，Send-safe）。**翻页由 UI 主动递增 `page`**——用户翻一页才取一页。**不要**让引擎一次批量翻 N 页（早期 `by:url` 批量版会一次开 5 个浏览器、UI 卡"加载中"，已回退删除 `paginate_by_url`）。

边界信号走 `has_more`（book_list/v0 响应 data 顶层有 `has_more` bool；`total_count` 实测恒为 10000 占位，**不可靠，别用作总页数**）。

**相关文件**：`crates/parse-book-source/src/engine/mod.rs`、`fanqie-web.v2.json`、`openspec/changes/list-has-more/`

## 反爬实测

### bilixs / Cloudflare

- bilixs 只锁搜索接口；headful 浏览器能解 CF managed 挑战。
- `cf_clearance` cookie **不绑 TLS 指纹**，可从浏览器交接给 reqwest 复用；但**绑 UA**——交接时 UA 必须一致。

**相关文件**：`crates/parse-book-source/src/fetch/`、记忆 `booksource-anti-scraping-findings`

### chromiumoxide 默认 `--enable-automation` 会让 CF 解挑战死循环卡死

chromiumoxide 的 `DEFAULT_ARGS` 强制带 `--enable-automation`，它让 Chrome 显示「受自动化控制」并改 `window.chrome`，是 Cloudflare managed challenge 识别 CDP 自动化的经典信号。实测：用户点过 Turnstile、CF reload 后会**反复重新挑战**，`cf_clearance` 永不签发，`solve` 轮询卡死到超时（诊断时 cookie 一直卡在 `cf_chl_*`，从不出现 `cf_clearance`）。

**正确做法**（仅 headful 解挑战路径）：
```rust
builder = builder.disable_default_args().hide().with_head();
```
- `disable_default_args()` 拔掉含 `--enable-automation` 的全部默认参数（其余多为噪声/性能项）；
- `.hide()` 补回 `--disable-blink-features=AutomationControlled`——现代 Chrome 里这个 blink 特征**原生**就把 `navigator.webdriver` 设为 false（无需再用 JS `Object.defineProperty` 覆盖，冗余）。

**不要**：
- 用 `enable_stealth_mode()` 全套——它伪造 WebGL（`NVIDIA GTX 1050` Windows D3D11）+ 插件，与本机真实环境（如 macOS）矛盾，UA↔WebGL 不一致反而是更易被指纹识别的信号。
- 动 headless 渲染路径（番茄流）——它保留默认参数已验证可用，解挑战的改动只加在 `if !headless` 分支。

**相关文件**：`crates/parse-book-source/src/fetch/browser/fetcher.rs`（`spawn_browser`）

### `disable_default_args()` 是「全有或全无」，会连带拔掉抑制首次运行体验(FRE)的参数 → Edge 弹欢迎登录模态卡死解挑战

上一条为去 `--enable-automation` 调了 `disable_default_args()`，但 chromiumoxide 这个开关**不能只去一个默认参数**——它把 `DEFAULT_ARGS`（24 项）**全部**拔掉。其中 `--disable-sync`/`--disable-default-apps`/`--disable-client-side-phishing-detection` 等是抑制浏览器**首次运行体验(FRE)**的关键。实测 Windows 上探测到的浏览器是 **Edge** 时，headful 解挑战会弹出「欢迎使用 Microsoft Edge / 同步登录(是，继续 / 否，注销我)」模态**挡住挑战页**，用户无从点「确认真人」→ 解挑战拿不到 `cf_clearance` → 下游 reqwest 重试 `HTTP 403`。headless 渲染路径(番茄)保留了默认参数，故无此问题。

**正确做法**：headful 分支 `disable_default_args()` 后，手动补回「`DEFAULT_ARGS` 去掉 `--enable-automation`」的等价集（常量 `HEADFUL_DEFAULT_ARGS`，关键是 `--disable-sync`）：
```rust
builder = builder.disable_default_args().hide().with_head();
for &arg in HEADFUL_DEFAULT_ARGS { builder = builder.arg(arg); }
```
- 补回的都是环境/性能/FRE 抑制项，**没有**自动化指纹信号（CF 只认 `--enable-automation`），故不破坏上一条的 CF 修复。
- 略去 `--lang=en_US`（保用户原生 UI 语言）与 `--enable-blink-features=IdleDetection`。
- `HEADFUL_DEFAULT_ARGS` 是 chromiumoxide **0.9.1** 的 `DEFAULT_ARGS` 镜像，**升级该依赖时需复核**这份列表。

**不要**：以为 `--no-first-run` 就够了——它只压住「首次运行」那一道，Edge 的同步登录 FRE 模态要靠 `--disable-sync` 才压得住。

**相关文件**：`crates/parse-book-source/src/fetch/browser/fetcher.rs`（`HEADFUL_DEFAULT_ARGS`、`spawn_browser`）

## novel-tts

### 模型钉版见 toolchain

kokoro-tts `0.3.1` 勿升（rc.12 砍 Intel Mac），见 [toolchain.md](toolchain.md)。模型文件（`kokoro-v1.1-zh.onnx`、`voices-v1.1-zh.bin`）自动从 GitHub 下载到 `~/.novel-tts/kokoro/`，HTTP Range 断点续传，`CancellationToken` 取消。

<!-- 随开发补充:新规则 DSL 前缀、新站点接入坑等 -->
