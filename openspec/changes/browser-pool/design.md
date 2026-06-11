## Context

`BrowserFetcher`(chromiumoxide 0.9)的渲染取页(`render_intercept`/`render_dom`)目前内部 `launch` 一个浏览器、渲染完关闭。`EscalatingFetcher::fetch_full` 的 render 分支每次调用都走一次。chromiumoxide `launch` 是秒级开销(起进程 + handler + 首页)。

现有保护:`BROWSER_LOCK`(进程内串行化渲染,避免并发开多个浏览器 / 番茄风控)、`RENDER_FAILED` 熔断(启动类失败后本会话停用、不再频闪)。这些要保留。

约束:仅 `browser` feature;失败优雅降级(该 op 不可用,不影响其它);无浏览器 / 纯净构建行为不变。

## Goals / Non-Goals

**Goals:**
- 渲染取页**复用常驻浏览器**:翻页时只开新 `Page`、导航目标 URL,不重启浏览器。
- `render_intercept`/`render_dom` 对外签名与失败语义不变。
- 为 P2(点击驱动翻页)预留:同一会话内可对一个 `Page` 多次拦截/操作。

**Non-Goals:**
- 不做多浏览器池(单个常驻 `Browser` 足够;番茄风控下渲染本就串行)。
- 不并发渲染(保留 `BROWSER_LOCK` 串行;并发 `Page` 留后续)。
- 不改 headless/headful 策略、不改解 CF 挑战路径(`solve`)。

## Decisions

**D1 — 单个常驻 `Browser` + per-render 新 `Page`。**
- `BrowserFetcher` 持有 `Arc<Mutex<Option<(Browser, JoinHandle)>>>`(或专门 `BrowserPool` 类型):首次渲染取页时 `launch` 建一次,存入;后续取页**复用该 `Browser`**,`browser.new_page(...)` 开新页、导航、拦截/读 DOM、用完 `page.close()`,**`Browser` 留着**。
- 备选(否决):`Page` 也复用(不每次新建)——SPA 状态/cookie 跨页残留易串味,新 `Page` 更干净;`new_page` 比 `launch` 快几个数量级,够了。

**D2 — 生命周期:懒启动 + 空闲超时/退出关 + 崩溃重建。**
- 懒启动:首个渲染请求才建浏览器(纯净/无渲染需求时零开销)。
- 关闭:app 退出时显式关(`Drop` 或退出钩子);可选空闲超时(N 分钟无渲染则关,省内存)。
- 重建:取/用 `Browser` 时若发现已断连(handler task 结束 / CDP 失败)→ 丢弃、重新 `launch`。
- 复用 `RENDER_FAILED`:启动类失败仍熔断本会话。

**D3 — handler task 与 `Send`。**
- chromiumoxide `Browser::launch` 返回 `(Browser, Handler)`,`Handler` 需常驻 `tokio::spawn` 驱动 CDP 事件循环;池要持有其 `JoinHandle`,关闭时一并取消。
- `Browser` 跨 await 复用,需保证 `EscalatingFetcher`/`BrowserFetcher` 的相关 Future 仍 `Send`(主程序在 `tokio::spawn` 内取页)。

**D4 — 保留串行与熔断。**
- `BROWSER_LOCK` 继续串行化「取池 → 开 Page → 渲染」整段(避免并发开 Page 触发番茄风控);池本身的取/建也在锁内,天然防并发重复 launch。

## Risks / Trade-offs

- [常驻浏览器进程占内存] → 空闲超时关闭(D2);仅在用过渲染的会话存在。
- [浏览器崩溃/断连后池里是僵尸] → 用前探活 + 重建(D2)。
- [handler task 泄漏] → 池持 `JoinHandle`,关闭时 `abort`。
- [跨 `Page` 状态串味] → 每次新 `Page`(D1),不复用 Page。
- [`Send` 回归] → 与 explore/search 修复同源教训:改完必 `cargo build` 主程序验证 `tokio::spawn` 仍编译。
