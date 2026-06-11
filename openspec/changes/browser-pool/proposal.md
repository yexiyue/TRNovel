## Why

渲染取页(explore/search 的 `render`)目前**每次取页都 launch 一个浏览器、渲染后关闭**(`EscalatingFetcher::fetch_full` 的 render 分支 → `BrowserFetcher::render_intercept`/`render_dom` 内部 `launch` 一次)。chromiumoxide 启动浏览器是**秒级**开销,带来两个问题:

1. **交互翻页体验差**:番茄书库 explore 是单页 + UI 递增 `page`,用户每翻一页就重启一次浏览器(几秒 loading + 系统栏闪),不顺滑。
2. **P2(by:click)做不了**:番茄 search 翻页(URL 不认页码)需要在**一次浏览器会话内**「拦第 1 页 → 点下一页 → 拦第 2 页」——本质是会话内复用浏览器翻多页,没有常驻浏览器无法实现。

## What Changes

- **常驻浏览器实例池**:首次渲染取页时懒启动一个浏览器并常驻(`Arc` 共享 + handler task 长跑);后续渲染取页**复用该浏览器、只开新 `Page` 导航目标 URL**,用完关 `Page`、留 `Browser`。翻页从「每页重启浏览器」变成「复用浏览器、只导航新 URL」。
- **生命周期管理**:空闲超时 / app 退出关闭浏览器;浏览器崩溃 / 断连时重建。保留现有 `BROWSER_LOCK` 串行语义(番茄风控下渲染仍串行)与 `RENDER_FAILED` 熔断。
- **集成**:`BrowserFetcher`/`EscalatingFetcher` 的 render 路径从「每次 launch」改为「从池取 `Browser`、开 `Page`」;对外接口(`render_intercept`/`render_dom`)签名与失败优雅降级语义**不变**。
- 向后兼容:无浏览器 / 纯净构建行为不变;仅 `browser` feature。

## Capabilities

### New Capabilities
- `browser-pool`: 常驻浏览器实例池——渲染取页复用同一浏览器(开新 `Page`)而非每次 launch/close,降低翻页延迟;同时是 P2(点击驱动翻页)在一次会话内翻多页的基础设施。

## Impact

- `crates/parse-book-source/src/fetch/browser/`:`BrowserFetcher` 持有常驻 `Browser`(`Arc<Mutex<Option<Browser>>>` 或专门 Pool 类型);`render_intercept`/`render_dom` 改从池取 `Browser`、开 `Page`;`launch` 收敛为「池空时建一次」。
- `EscalatingFetcher`:render 分支复用池,不再每次 launch。
- 依赖:chromiumoxide 0.9(已在 `browser` feature)。
- 风险:常驻浏览器进程内存;崩溃/断连重建;handler task 生命周期;并发 `Page` 与番茄风控的取舍(默认仍串行)。
