## Why

部分网络书源(如 bilixs)对**搜索**等端点启用了 Cloudflare Managed Challenge:纯 reqwest 取页拿到的是 403 挑战页而非内容,现有引擎把它当普通失败,既访问不了也说不清原因。现场实测已证明一条可行路径:用**用户系统里已装的真实浏览器**(Chrome / Edge 均验证通过)以 headful 方式可解开该挑战并签发 `cf_clearance`,且该 cookie **不绑 TLS 指纹**——把它(连同浏览器真实 UA)交回 reqwest,即可继续以普通快请求访问受保护端点。

本 change 把这条已验证路径产品化:在**不引入任何 C 依赖、不破坏单文件分发**的前提下,为受反爬保护的书源提供「能自动就自动、必要时一次真人点击」的访问能力,并在浏览器不可用时优雅降级。读取链路(目录/正文/浏览)本就开放,本 change 主要恢复的是「在受挑战站点上搜索」。

## What Changes

- 新增 **`BrowserFetcher`**(实现既有 `trait Fetcher`):探测系统 Chromium 系浏览器,以**持久化 profile** + headful 启动,导航到被挑战的 URL,充当「cookie 烤箱」解挑战。
- **协助式解挑战**:轮询 `cf_clearance`——非交互路径静默完成(用户基本无感);当 Cloudflare 自适应升级为 Turnstile 勾选框时,把浏览器窗口提到最前并在 TUI 提示用户点一下「确认您是真人」。**绝不模拟点击**(合成点击会被 Turnstile 识别)。
- **交接**:通过 CDP 读出明文 `cf_clearance` 与浏览器**真实 UA**,注入 reqwest 的 cookie store;后续请求 MUST 使用该真实 UA(cf_clearance 绑 UA)。clearance 缓存至过期,过期或再遇挑战时重新求解。
- **挑战检测**:`ReqwestFetcher` 识别 `cf-mitigated: challenge` 及挑战页特征,据此触发向 `BrowserFetcher` 升级;`diagnose` / `doctor` 给出精确诊断而非笼统失败。
- **降级链**:未找到可用浏览器时回退到「手贴 cookie」并据实报告;`search` 等能力可被书源声明为**可选**,不可用时引导改用浏览(explore)。
- 书源配置新增取页模式声明(`http.fetcher: auto | reqwest | browser`)与受影响能力标注;app 侧 profile 落在 `~/.novel/browser-profile/`。
- 依赖新增 `chromiumoxide`(纯 Rust CDP 客户端)与浏览器路径探测,**以 `browser` feature 门控**,默认构建不变重。

## Capabilities

### New Capabilities
- `book-source-challenge-detection`: 识别 Cloudflare / 反爬挑战响应(`cf-mitigated` 头、挑战页特征),据此触发取页升级、把受阻能力降级为可选、并产出精确诊断。
- `book-source-browser-fetcher`: 基于系统真实浏览器的取页适配器——探测/启动、持久 profile、协助式解挑战(非交互静默 + 交互式提示用户点击)、CDP 读取明文 cookie 与真实 UA、UA 绑定地交回 reqwest、无浏览器时降级。

### Modified Capabilities
<!-- openspec/specs/ 为空(尚无已合并 spec)。ai-friendly-book-source 的 book-source-requests 已为反爬预留 cookie 注入点;本 change 在其之上新增能力,不修改其已发布需求,故此处不登记修改。 -->

## Impact

- **crates/parse-book-source**:`fetch` 新增 `BrowserFetcher`(behind `trait Fetcher`);`engine` 增加「撞挑战 → 升级浏览器」编排;`source` 增加取页模式与能力可选性配置;`verify` 增加挑战检测与诊断项。新增依赖 `chromiumoxide`(纯 Rust)+ 浏览器路径探测,建议 `browser` feature 门控。
- **trnovel(app)**:Engine 构造接线 BrowserFetcher;TUI 协助式解挑战提示与窗口前置;profile 目录 `~/.novel/browser-profile/`;`doctor` 展示挑战诊断与降级状态。
- **分发**:零新增 C 依赖,单文件 cargo-dist 不受影响;浏览器是用户既有安装,不链接进二进制。
- **平台**:浏览器探测与窗口前置按 macOS / Windows(Edge 必备,零下载默认)/ Linux 分别实现;纯 Safari/Firefox 用户走降级链。
- **非目标(明确排除,留给后续 change)**:wreq / TLS 指纹伪装、FlareSolverr 外接、打码服务、规则级 JS 引擎(boa)与声明式解密算子。
