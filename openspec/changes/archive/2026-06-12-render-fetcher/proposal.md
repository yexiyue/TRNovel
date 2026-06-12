## Why

现有 `browser` fetcher(`crates/parse-book-source/src/browser.rs`,chromiumoxide headful)只是「cookie 烤箱」——解 Cloudflare 挑战、签发 `cf_clearance` 交回 reqwest,**不取 JS 渲染后的内容**。这导致 SPA 渲染型站点无法接入:reqwest 只拿到空壳。典型受害者是番茄网页搜索——接口走 ByteDance `sec_sdk` 签名(`a_bogus`/`msToken`,绑浏览器上下文、不可外带),结果只在客户端 JS 渲染、`book_id` 只在内存。

PoC(已合入 `browser.rs::render_intercept`/`render_and_probe` + `examples/spa_search_poc.rs`)实测确认:让真浏览器跑 SPA 自身 JS(签名由浏览器+SPA 完成,我们从不复刻签名),再用 CDP 拦截目标 API 的响应体,即可拿到含 `book_id` 的会员态 JSON;且 **headless 与 headful 结果一致**(`sec_sdk` 不拒签无头)。把这条能力产品化,既解决番茄搜索,也一并解除「整页正文由 JS 渲染 → 暂不支持」的架构边界(任何 SPA 小说站受益)。

## What Changes

- **新增渲染型取页能力**:`BrowserFetcher` 暴露「导航 → 跑站点 JS → 就绪等待 → 返回渲染后 DOM」与「CDP 拦截指定 API 响应体」两种取页(PoC 方法转正、清理为正式 API)。
- **渲染默认走 headless**(快、无窗口);仅登录与解 CF/Turnstile 挑战保持 headful(需真人交互)。失败一律优雅降级(无浏览器/被拒签 → 该 op 不可用,行为同现状)。
- **book-source schema 加 per-op 渲染开关**(放在 `Request` 上):`render`(bool)、`readyFor`(就绪 CSS 选择器)、`interceptApi`(目标响应 URL 子串/正则)。默认关闭 → **零行为变化**;仅显式开启的 op(典型 `search`)走渲染路径。
- **engine 路由**:开启渲染的 op 取页改走渲染 fetcher;拦截到的 JSON 用现有 `via:"json"` 规则解析(`book_id` → `bookUrl` 拼接);PUA 字段用现有 `fontMap` clean 解码(注意搜索字体与正文字体不同,需各自的表),或读渲染后 DOM 的已解码文本。
- 全部仅在 `browser` feature 下编译;纯净构建/无浏览器环境**行为不变**。
- 落地验证:`fanqie-web.v2.json` 的 `search` 标 `render` + `interceptApi: "search_book/v1"`,doctor 的搜索项转 ✓。

## Capabilities

### New Capabilities
- `render-fetcher`: 用受控浏览器渲染 SPA / JS 站点并取「渲染后 DOM」或「CDP 拦截的 API 响应体」的取页能力;含 headless/headful 策略、就绪等待、按 op 显式开启、失败降级。

### Modified Capabilities
<!-- 无既有 spec 的需求变更;schema 的新增字段与 engine 路由属新能力的一部分。 -->

## Impact

- `crates/parse-book-source/src/browser.rs`:渲染 + CDP 拦截方法(转正 PoC)。
- `crates/parse-book-source/src/fetch.rs` / `engine.rs`:取页路由,按 op 渲染开关。
- `crates/parse-book-source/src/source.rs`(schema):`Request` 新增 `render`/`readyFor`/`interceptApi`(向后兼容,默认关闭)。
- 依赖:chromiumoxide 0.9.1(已在 `browser` feature)。
- 书源数据:`fanqie-web.v2.json` 接搜索;`booksource-generator` skill 文档更新(SPA 站点可做搜索)。
- 风险:每次渲染开一次浏览器(秒级,搜索低频可接受);自动化指纹被 `sec_sdk` 识别(PoC 未被拒签,但需运行期兜底);CDP `getResponseBody` 时序(响应缓冲仍在时取)。
