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
- 常驻池是**进程级单例** `static RENDER_POOL: Mutex<Option<Resident>>`(`Resident { browser, handler, headless }`),**不是** `BrowserFetcher` 的实例字段。首次渲染取页时 `spawn_browser` 建一次,存入;后续取页**复用该 `Browser`**,`browser.new_page(...)` 开新页、导航、拦截/读 DOM、用完 `page.close()`,**`Browser` 留着**。
- **为何全局单例(落地时修正,原稿写「`BrowserFetcher` 持有」是错的)**:所有书源共享同一持久 profile(`~/.novel/browser-profile`),`build_engine` 每次新建一个 `BrowserFetcher`。若池是实例字段,多书源/路由回退栈下两个实例各自的常驻浏览器会同时存活、互抢 profile 的 `SingletonLock`(后建者 `spawn_browser` 无条件删锁)→ profile 数据竞争。`BROWSER_LOCK` 只保证「同一时刻只一个浏览器在启动/渲染」,常驻化把「存活」与「持锁」解耦后跨实例存活并存不受锁保护,只有全局单例能根治。
- 备选(否决):`Page` 也复用(不每次新建)——SPA 状态/cookie 跨页残留易串味,新 `Page` 更干净;`new_page` 比 `spawn_browser` 快几个数量级,够了。

**D2 — 生命周期:懒启动 + 退出显式关 + 崩溃重建。**
- 懒启动:首个渲染请求才建浏览器(纯净/无渲染需求时零开销)。
- 关闭:池是 static、进程退出**不触发 `Drop`**,故 app 在退出点显式 `parse_book_source::shutdown_render_pool().await`(`src/lib.rs`);headful solve/login 前也调它腾 profile。**不能**给 `BrowserFetcher` 加 `Drop` 拆全局池——任一 engine drop 会杀掉别 engine 仍在用的浏览器。空闲超时:Non-Goal,未实现(故意)。
- 重建:`new_pool_page` 取 `Browser` 时 `handler.is_finished()` 快路检查 + **开页(`new_page`)套 8s 超时**(死浏览器的 `new_page` 要等 CDP 默认 30s 才报错且独占 `BROWSER_LOCK`);失败/超时 → 丢弃、重 `spawn_browser`,至多重建一次。
- 复用 `RENDER_FAILED`:启动类失败仍熔断本会话。

**D3 — handler task 与 `Send`。**
- chromiumoxide `Browser::launch` 返回 `(Browser, Handler)`,`Handler` 需常驻 `tokio::spawn` 驱动 CDP 事件循环;`Resident` 持其 `JoinHandle`,拆除时一并 `abort`。
- `Browser` 跨 await 复用,需保证 `EscalatingFetcher`/`BrowserFetcher` 的相关 Future 仍 `Send`(主程序在 `tokio::spawn` 内取页)。

**D4 — 保留串行与熔断。**
- `BROWSER_LOCK` 继续串行化「取池 → 开 Page → 渲染」整段(避免并发开 Page 触发番茄风控);池的取/建/拆都在锁内,锁序固定 `BROWSER_LOCK → RENDER_POOL`,天然防并发重复 launch。

## Risks / Trade-offs

- [常驻浏览器进程占内存] → 仅在用过渲染的会话存在;退出显式关(D2)。空闲超时为 Non-Goal。
- [浏览器崩溃/断连后池里是僵尸] → 开页失败/超时触发拆除重建(D2;`is_finished()` 不可靠故靠开页超时兜底)。
- [多 engine 共享 profile 抢 `SingletonLock`] → 池提为进程级单例(D1)。
- [handler task 泄漏] → `Resident` 持 `JoinHandle`,拆除时 `abort`。
- [static 不触发 `Drop` → 退出泄漏浏览器进程] → app 退出点显式 `shutdown_render_pool()`(D2)。
- [跨 `Page` 状态串味] → 每次新 `Page`(D1),不复用 Page。
- [`Send` 回归] → 与 explore/search 修复同源教训:改完必 `cargo build` 主程序验证 `tokio::spawn` 仍编译。
