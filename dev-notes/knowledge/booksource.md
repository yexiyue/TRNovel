# 书源 / parse-book-source / TTS

## 概览

`crates/parse-book-source`（Legado 风格书源解析引擎、规则 DSL、反爬/渲染抓取）与 `crates/novel-tts`（Kokoro TTS）的项目特有约束。重点是番茄（fanqienovel.com）这类 SPA + 签名站点的接入路线。

## 规则引擎

### 抓取分层 fetch / render

`fetch_checked` 是纯 reqwest（拿静态 HTML / 普通 API），**SPA 站点拿不到数据**（首屏是空壳，数据靠 JS 拉）。SPA 走 **render 通道**（CDP 驱动真浏览器渲染 + `interceptApi` 拦签名 API 响应），由 `fetch/browser/` 下的 fetcher + EscalatingFetcher 实现。

**op 上的 render 三字段**：`render`（开渲染）/`readyFor`（等待选择器/条件）/`interceptApi`（拦哪个 API 的响应当数据源）。重构后 `ExploreOp`/`SearchOp` 都对齐了这三字段（早期只有 search 的主 Request 有）。

**相关文件**：`crates/parse-book-source/src/fetch/browser/`、`src/source/op.rs`

## 番茄（fanqienovel.com）接入

### 签名不可破解，render 让浏览器自己签

番茄 API 带字节跳动 secsdk 签名：`a_bogus`（每请求级，依赖 DOM/BOM/Worker 运行时算）+ `msToken`（会话级）。**boa 跑不了**这段 JS（需要浏览器环境），别想着把签名算法搬进 boa/Rust 复刻——验证过，不可行。唯一可行路线是 **render**：驱动真浏览器，让它自己带签名发请求，我们只 `interceptApi` 拦响应 JSON。

**不要做**：试图逆向 `a_bogus`/`msToken` 算法在 Rust 侧复刻。

### 两套 API：网页端 vs App 端

- **网页端**（fanqienovel.com）：无签名负担的部分有限——只前 ~10 章完整,其余试读 / 需会员。
- **App 端**：全本免费但需签名 + AES 解密。
- 字体解密是两端通用能力。

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

**相关文件**：`crates/parse-book-source/src/fetch/browser/fetcher.rs`（`launch`）

## novel-tts

### 模型钉版见 toolchain

kokoro-tts `0.3.1` 勿升（rc.12 砍 Intel Mac），见 [toolchain.md](toolchain.md)。模型文件（`kokoro-v1.1-zh.onnx`、`voices-v1.1-zh.bin`）自动从 GitHub 下载到 `~/.novel-tts/kokoro/`，HTTP Range 断点续传，`CancellationToken` 取消。

<!-- 随开发补充:新规则 DSL 前缀、新站点接入坑等 -->
