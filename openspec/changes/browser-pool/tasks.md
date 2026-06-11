## 1. 池结构与生命周期

- [x] 1.1 `fetch/browser`:引入常驻浏览器持有(`BrowserFetcher.render_pool: Mutex<Option<Resident>>`,`Resident{browser,handler,headless}`),懒启动(`new_pool_page` 首次 spawn)+ 用前探活(`handler.is_finished()`)/断连重建(开页失败拆掉重建一次)+ 关闭(`Drop for BrowserFetcher` 同步 abort+drop,`kill_on_drop` 杀子进程)
- [x] 1.2 `launch` 拆为 `spawn_browser`(纯启动,不锁)+ `launch_ephemeral`(headful solve/login 用,先 `teardown_render_pool` 腾 profile);常驻 handler `JoinHandle` 由 `Resident` 持有,拆除时 `abort`

## 2. 渲染取页改走池

- [x] 2.1 `render_intercept`/`render_dom` 从池取 `Browser`、`new_pool_page` 开新 `Page` 导航,用完 `page.close()` 留 `Browser`;`*_inner` 改为接 `&Page` 的 `render_dom_page`/`intercept_page`;**对外签名与失败优雅降级语义不变**(测试 `render_request_degrades_without_browser` 仍绿)
- [x] 2.2 保留 `BROWSER_LOCK` 串行 + `RENDER_FAILED` 熔断;render 整段持 `BROWSER_LOCK`,池的取/建/拆都在锁内(锁序 `BROWSER_LOCK → render_pool`),天然防并发重复 launch

## 3. 验证

- [x] 3.1 `cargo build` 主程序(explore/search 在 `tokio::spawn` 内调用,验 `Send` 不回归)——通过(render 跨 await 持两把 tokio guard,均 `Send`)
- [x] 3.2 `test --all-features`(141 passed)+ `clippy -D warnings` + `doc -D warnings` + `fmt --check`,全绿
- [ ] 3.3 doctor / 手动:番茄书库 explore 连续翻页,确认只开一次浏览器、翻页明显变快(**待用户浏览器环境实跑**——沙箱无浏览器/网络,无法在此验证)
