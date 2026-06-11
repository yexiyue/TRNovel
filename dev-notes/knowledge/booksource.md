# 书源 / parse-book-source / TTS

## 概览

`crates/parse-book-source`（Legado 风格书源解析引擎、规则 DSL、反爬/渲染抓取）与 `crates/novel-tts`（Kokoro TTS）的项目特有约束。重点是番茄（fanqienovel.com）这类 SPA + 签名站点的接入路线。

## 规则引擎

### 抓取分层 fetch / render

`fetch_checked` 是纯 reqwest（拿静态 HTML / 普通 API），**SPA 站点拿不到数据**（首屏是空壳，数据靠 JS 拉）。SPA 走 **render 通道**（CDP 驱动真浏览器渲染 + `interceptApi` 拦签名 API 响应），由 `fetch/browser/` 下的 fetcher + EscalatingFetcher 实现。

**op 上的 render 三字段**：`render`（开渲染）/`readyFor`（等待选择器/条件）/`interceptApi`（拦哪个 API 的响应当数据源）。重构后 `ExploreOp`/`SearchOp` 都对齐了这三字段（早期只有 search 的主 Request 有）。

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
- **路由 = dom-presence（不内省规则 via）**：`Rule` 是枚举、组合规则可混 via，逐规则内省太重。引擎实际按「**抓到 DOM 就对 DOM 求值 `totalPages`，否则对 body**」路由（`eval_total_pages`）。因为作者配 `ready_for`（→抓 DOM）正是 totalPages 要 DOM 的 opt-in；via:json 总数 / 非 render 的 css 总数则无 DOM、打 body（便宜档白送）。
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

### explore 是 URL 驱动，search 是点击驱动

- **explore 书库** `/library/all/page_N`：URL 驱动——翻页 URL 变,直接导航该 URL 即渲染第 N 页。API `book_list/v0`（`page_index=N-1` 0 基）。**`by:url` 翻页可用**。
- **search** `/search/{词}`：SPA **不认 URL 页码**（path/query 变体实测都回第 1 页），只能点分页器"下一页"触发 `page_index` 递增。API `search_book/v1`。`by:url` 对 search 无效，需 `by:click`（render 会话内拦 N 个 + 每页点下一页，P2 范围）。

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
