## 1. 池结构与生命周期

- [ ] 1.1 `fetch/browser`:引入常驻浏览器持有(`Arc<Mutex<Option<(Browser, JoinHandle)>>>` 或专门 `BrowserPool` 类型),懒启动 + 用前探活/断连重建 + 关闭(`Drop` / app 退出钩子;可选空闲超时)
- [ ] 1.2 `launch` 收敛为「池空时建一次」;handler task 由池持有 `JoinHandle`,关闭时 `abort`

## 2. 渲染取页改走池

- [ ] 2.1 `render_intercept`/`render_dom` 从池取 `Browser`、`new_page` 导航,用完 `page.close()`;**对外签名与失败优雅降级语义不变**
- [ ] 2.2 保留 `BROWSER_LOCK` 串行 + `RENDER_FAILED` 熔断;取/建池在锁内,天然防并发重复 launch

## 3. 验证

- [ ] 3.1 `cargo build` 主程序(explore/search 在 `tokio::spawn` 内调用,验 `Send` 不回归)
- [ ] 3.2 `test --all-features` + `clippy -D warnings` + `doc` + `fmt`,全绿
- [ ] 3.3 doctor / 手动:番茄书库 explore 连续翻页,确认只开一次浏览器、翻页明显变快(待用户浏览器环境实跑)
