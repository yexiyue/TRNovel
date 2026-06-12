## Context

`browser` feature 现有 `BrowserFetcher`(chromiumoxide 0.9.1)只做「cookie 烤箱」:headful 解 CF 挑战、签发 `cf_clearance` 交回 reqwest。它**不取 JS 渲染后的内容**,故 SPA 站点(番茄网页搜索)无法接入。

PoC(`browser.rs::render_intercept`/`render_and_probe` + `examples/spa_search_poc.rs`)对番茄搜索实测确认:
- 搜索接口 `/api/author/search/search_book/v1` 走 `sec_sdk` 签名(`a_bogus`/`msToken`),绑浏览器上下文,**curl replay 返回空**;结果 DOM `.search-book-item` **无 `book_id`/`href`**(只在内存)。
- 让真浏览器跑 SPA → SPA 自己签名+拉取+渲染;**CDP 拦截 `search_book/v1` 响应** → 拿到含明文 `book_id` 的会员态 JSON(10 本)。
- **headless 与 headful 结果一致**,`sec_sdk` 不拒签无头。
- 搜索页字体(`c207f68a84deae3.woff2`)≠ 正文字体(`dc027189e0ba4cd.woff2`),`book_name`/`author`/`abstract` 各带 PUA、映射不同;`book_id`/`category`/`thumb_url` 明文。

## Goals / Non-Goals

**Goals:**
- 让书源能取「JS 渲染后」的内容:两种取页 —— 返回渲染后 DOM(方式 A),或 CDP 拦截指定 API 响应体(方式 B)。
- 按 op 显式开启(默认关闭,零行为变化);渲染默认 headless,登录/解挑战才 headful。
- 把番茄搜索接通(doctor 搜索项转 ✓);失败一律优雅降级。
- 不复刻任何站点签名——签名由真浏览器+真 SPA 完成。

**Non-Goals:**
- 不做通用爬虫框架;只在书源明确开启的 op 上渲染。
- 不解决「字体跨页面/会话轮换」的通用动态字体还原(本 change 只把搜索字体当作可单独生成/或读 DOM 已解码文本来处理)。
- 不改纯净构建/无浏览器环境的行为。

## Decisions

**D1 — 两种取页都要,but 番茄搜索用方式 B。**
- 方式 A(渲染后 DOM):导航 → 等 `readyFor` 选择器出现 → 返回 `document.documentElement.outerHTML`,交给现有 CSS 规则。适合「DOM 里有完整数据」的 SPA。
- 方式 B(CDP 拦截):`Network.enable` + 监听 `EventResponseReceived`,URL 含 `interceptApi` 即 `Network.getResponseBody` 取响应体,交给现有 `via:"json"` 规则。
- 番茄搜索结果 DOM **无 book_id**,**必须方式 B**。两者都保留:书源用 `interceptApi` 选 B、用 `readyFor`(无 `interceptApi`)选 A。
- 备选:只做方式 A(否决——拿不到 book_id);只做方式 B(否决——不是所有站都有干净 API,DOM 站需要 A)。

**D2 — 渲染默认 headless,仅登录/解挑战 headful。**
- PoC 实测 headless 不被 `sec_sdk` 拒签且结果一致;headless 更快、无窗口打扰。
- 登录(用户手动操作)与 CF/Turnstile(检测合成点击)仍 headful(现状不变)。
- `render`/`intercept` 取一个 `headless: bool` 参数,engine 对渲染 op 默认传 `true`。

**D3 — schema:per-op 渲染开关放在 `Request` 上。**
- 新增 `render: bool`(默认 false)、`readyFor: Option<String>`(就绪 CSS 选择器)、`interceptApi: Option<String>`(目标响应 URL **子串**匹配,非正则;番茄场景子串足够)。
- 注:render 三字段只挂在 `Request`,目前仅 **search 主请求**生效;content/toc/bookInfo/explore 及所有 prelude 步骤恒为非渲染直取(future 可提升到这些 op)。
- 默认全空 → 现状(reqwest 直取),**逐字节向后兼容**。`search/explore/content` 的 `request` 均可开启。
- 备选:放 `http` 全局(否决——渲染是 per-op 的,搜索要渲染但目录/正文未必)。

**D4 — engine 路由 + 结果解析。**
- engine 取页时若该 op 的 `request.render` 为真 → 走 `BrowserFetcher` 渲染/拦截路径(需 browser 可用 ∧ 授权),否则 reqwest(现状)。
- 方式 B 的响应体 = 字符串,直接喂现有 `list`/`item` 的 `via:"json"`(`$.data...`);`book_id` → `bookUrl`(template 拼 `/page/{book_id}`)。
- PUA 字段:**搜索字体 ≠ 正文字体**,故搜索结果的 `name`/`author` 需各自的 fontMap(`gen-fontmap c207...woff2` 生成,内联进搜索 item 的 clean);或方式 A 直接读渲染后 DOM 文本(浏览器已解码)再按序与方式 B 的 book_id 对齐(更稳但实现复杂)。本 change 先采「搜索单独 fontMap」,DOM-对齐留作后续优化。

**D5 — CDP 拦截时序。**
- 先 `new_page("about:blank")` → `Network.enable` → 挂 `event_listener` → 再 `goto(url)`,避免错过 SPA 启动即发的请求。
- `getResponseBody` 在 `responseReceived` 后轮询取(body 可能稍后缓冲完);带总超时,超时即 `Err` 降级。

## Risks / Trade-offs

- [自动化指纹被 `sec_sdk` 识别拒签] → PoC headful+headless 均未被拒;运行期失败即降级(搜索不可用,不影响读),不阻塞主链路。复用养过的登录 profile 进一步降风险。
- [每次渲染开一次浏览器,秒级延迟] → 仅对显式开启的低频 op(搜索);可后续加浏览器实例复用/池化。
- [字体按页面类型/会话轮换] → 本 change 用搜索单独 fontMap;若实测搜索字体会轮换,则升级为「方式 A 读 DOM 已解码文本」彻底免表(已在 D4 留路径)。
- [CDP `getResponseBody` 取不到 body(已被清理/二进制)] → 轮询 + 总超时;取不到即降级。
- [headless 在某些站被识别为 bot] → 取页模式可回退 headful(留参数)。

## Migration Plan

1. `browser.rs`:把 PoC 的 `render_intercept`/`render_and_probe` 清理为正式 API(去掉 PoC 味、补错误语义),保留 `examples/spa_search_poc.rs` 作手动验收。
2. schema 加字段(默认关闭)→ 现有书源零改动。
3. engine 路由开关 → 仅渲染 op 受影响。
4. `fanqie-web.v2.json` 接搜索 + 生成搜索 fontMap;`trn doctor` 验证搜索 ✓。
5. 回滚:书源去掉 `render` 字段即回到现状;feature 不变。

## Open Questions

- 搜索字体 `c207f68a84deae3.woff2` 是否跨会话稳定?(决定「单独 fontMap」vs「读 DOM 已解码文本」)——实现期再跑两个关键词比对。
- 是否需要浏览器实例复用(避免每搜一次开一次)?先按「开一次」做,后续按体验优化。
