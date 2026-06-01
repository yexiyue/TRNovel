## 1. 挑战检测与诊断(无浏览器也立即受益,先落)

- [x] 1.1 在 `fetch`/`error` 增加挑战判定:识别 `cf-mitigated: challenge` 头与挑战页特征(`_cf_chl_opt`、`/cdn-cgi/challenge-platform/`、`Just a moment`),封装为 `is_challenge(resp/body) -> bool` 与一个 `FetchError::Challenged` 变体
- [x] 1.2 `ReqwestFetcher` 命中挑战时返回 `Challenged`(不把 body 当内容)
- [x] 1.3 `verify`/`diagnose` 把「被挑战」呈现为精确状态(被 Cloudflare 挑战、需浏览器/改用浏览),`doctor` 据此展示而非笼统 ✗
- [x] 1.4 离线单测:喂保存的挑战页 HTML + `cf-mitigated` 头,断言被识别为挑战;喂正常页断言不误判

## 2. 依赖、feature 门控与浏览器探测

- [x] 2.1 `crates/parse-book-source/Cargo.toml` 增加 `browser` feature,门控 `chromiumoxide` 等依赖;默认 feature 不含
- [x] 2.2 实现按平台的系统浏览器探测(macOS `/Applications`、Windows 注册表/`%ProgramFiles%`/`%LOCALAPPDATA%`、Linux `which`),覆盖 Chrome/Edge/Brave/Chromium/Vivaldi;返回可执行路径或「未找到」
- [x] 2.3 单测/可手验:探测在本机定位到至少一个浏览器;无浏览器时返回 None 不 panic

## 3. BrowserFetcher 核心(cookie 烤箱 · 非交互路径)

- [x] 3.1 新增 `fetch::browser` 模块与 `BrowserFetcher`(实现 `trait Fetcher`),持久 profile 目录参数(默认 `~/.novel/browser-profile/`)
- [x] 3.2 headful 启动浏览器 + `--remote-debugging-port=0`,经 CDP 端点发现实际端口并连接(chromiumoxide)
- [x] 3.3 导航被挑战 URL,轮询 `cf_clearance`;非交互路径在宽限期内取得即静默成功
- [x] 3.4 CDP `Network.getAllCookies` 读明文 `cf_clearance`(+其它 cf cookie),`/json/version` 读真实 UA
- [x] 3.5 子进程生命周期守卫:Drop/退出必杀;同一 profile 单实例(profile 锁/串行化)
- [x] 3.6 所有等待设总超时上限,超时返回失败(不无限轮询)

## 4. 协助式交互解挑战 + UA 绑定

- [x] 4.1 超宽限期未解开时,CDP `Runtime.evaluate` 检测 Turnstile 勾选框是否可见
- [x] 4.2 可见则 `Page.bringToFront` 窗口前置,并通过回调/事件向上层请求「提示用户点击」;继续轮询直至成功或总超时
- [x] 4.3 明确不实现任何 CDP 合成点击(代码注释 + 测试约束)
- [x] 4.4 `BrowserFetcher` 求解结果回传 `cf_clearance` + 真实 UA;后续请求强制用该 UA(UA 绑定)

## 5. 取页编排:升级与降级

- [x] 5.1 实现 `EscalatingFetcher { reqwest, browser }`(装饰器/责任链):reqwest 撞 `Challenged` 且 browser 可用时升级求解,再用 reqwest(带 clearance+真实 UA)重试原请求
- [x] 5.2 clearance 缓存:有效期内复用,过期或再撞挑战时重新求解
- [x] 5.3 降级链:无浏览器/解不开/用户取消 → 用 `http.cookies` 手贴 clearance;否则该能力报不可用并引导浏览
- [x] 5.4 `source` 增加 `http.fetcher: auto|reqwest|browser` 与能力可选性声明的反序列化;`Engine` 据此装配 Fetcher
- [x] 5.5 升级门控取「书源声明 ∧ 用户授权」交集:未授权或 `reqwest` 模式时一律不启动浏览器、走降级链(D12)

## 6. app 接线与 TUI

- [x] 6.1 app Engine 构造按 feature/配置接入 `EscalatingFetcher`;profile 落 `~/.novel/browser-profile/`
- [ ] 6.2 TUI:把「请在弹出的浏览器里点一下『确认您是真人』」做成 Modal/提示,接 4.2 的请求;支持取消(→降级)
- [x] 6.3 `doctor` 展示挑战诊断与降级状态(被挑战/已解/需点击/不可用)
- [ ] 6.4 app 级浏览器辅助授权:配置项 + 首次需要时询问「本次/总是/拒绝」并记住,持久化到 `~/.novel`;接 5.5 的用户授权门控

## 7. bilixs 书源修正与校验

- [x] 7.1 `test-novels/bilixs.v2.json`:search `list.select` 改为 `.module-search-item`,并标注 search 依赖浏览器辅助(`http.fetcher`/能力可选)
- [ ] 7.2 端到端手验(本机带浏览器):`doctor` 对 bilixs 搜索从 ✗ 变为「解挑战后成功 / 或提示点击后成功」

## 8. 收尾与质量门

- [x] 8.1 `openspec validate browser-fetcher --strict` 通过
- [x] 8.2 `cargo test --workspace`(含新增离线测试)、`clippy --all-targets --all-features -D warnings`、`fmt --check`、`doc` 全绿
- [x] 8.3 更新文档(docs 反爬/书源章节:能力分层、协助式解挑战、降级链、`http.fetcher` 说明)
