## Context

`ai-friendly-book-source` 重写后,取页被抽象为 `trait Fetcher`(端口-适配器),默认实现 `ReqwestFetcher`;design D8/D10 已明确「反爬后端只是另一个 `Fetcher` 适配器」,并预留了 `http.cookies` 注入位与 `http.warmup`。本 change 填这个口子。

现场实测(对 bilixs.com,2026-06-01,见项目记忆 `booksource-anti-scraping-findings`)确立了关键事实:

- bilixs 的 Cloudflare **Managed Challenge 只锁在 `/search.html` 等搜索路径**;目录/正文/浏览全 200。是**路径级 WAF 规则**(同一 plain reqwest,catalog 200、search 403)⇒ **TLS 指纹伪装(wreq)对它无效**。
- **系统真实浏览器 headful**(Chrome 146、Edge 148 均验证)无需任何伪装即可几秒解开挑战,签发 `cf_clearance`;**headless 没解开**。
- 把 CDP 读到的明文 `cf_clearance` + 浏览器**真实 UA** 交给 curl(指纹≠Chrome,等价 reqwest)→ **HTTP 200 + 真实结果** ⇒ **cf_clearance 不绑 TLS,可交回 reqwest**。
- 挑战**自适应**:风险低跑非交互 JS(无需点击),风险高升级为 **Turnstile 勾选框**(挑战页模板含 `cf-turnstile`/`cb-lb`);合成点击会被识别,**必须真人点**。

约束:TRNovel 是 cargo-dist 单文件 TUI,曾被 onnxruntime 的 C 运行时链接坑过 ⇒ **本 change 不得引入新的 C 依赖**。

## Goals / Non-Goals

**Goals:**

- 让受 Cloudflare 挑战保护的书源端点(主要是搜索)可访问:**能自动就自动,Cloudflare 自适应升级时才需用户一次点击**。
- 复用用户**已装**的系统浏览器,零额外下载、零新增 C 依赖、不破坏单文件分发。
- 撞挑战 → 升级浏览器 → 解挑战 → 交回 reqwest 全自动编排;`cf_clearance` 缓存复用。
- 无浏览器/解不开时**优雅降级**并据实诊断,绝不静默失败或假成功。

**Non-Goals:**

- 不做 wreq/TLS 指纹伪装、不接 FlareSolverr、不接打码服务。
- 不在本 change 做规则级 JS 引擎(boa)与声明式解密算子(留给后续 change B)。
- 不追求「保证零点击」全自动——自适应挑战做不到,本设计明确接受偶发真人点击。
- 不做内置/下载 Chromium(作为降级链里的备选记入 Open Questions,不在本期实现)。

## Decisions

### D1:CDP 客户端选 `chromiumoxide`(纯 Rust),不选 headless_chrome / fantoccini / wreq

- `chromiumoxide`:async/tokio 原生、走 DevTools 协议、底层 tokio-tungstenite,**无 C 依赖**,可启动或附着浏览器。契合现有 async 栈与单文件分发。
- 否决 `fantoccini`(WebDriver,还要额外装 chromedriver);`headless_chrome`(同步 API,不贴合);`wreq`(实测对路径级挑战无效,且引入 BoringSSL C 依赖);FlareSolverr(需用户跑 Docker,UX 不可接受)。

### D2:复用系统浏览器,不内置/下载 Chromium

- 探测系统已装的 Chromium 系浏览器(Chrome / Edge / Brave / Chromium / Vivaldi):macOS 扫 `/Applications/*.app`,Windows 查注册表/`%ProgramFiles%`/`%LOCALAPPDATA%`(**Edge 必装,零下载默认**),Linux `which chromium google-chrome`。
- 相比下载 chromium:零下载(~150MB×平台)、**真实指纹更易过 Cloudflare**、不触发杀软(静默下载+启动 .exe 像木马)、不用维护版本。
- 代价:纯 Safari/Firefox 用户没有 Chromium 系 ⇒ 走降级链(D8)。

### D3:「Cookie 烤箱」——浏览器只为签发 cf_clearance,reqwest 干活

撞挑战时升级浏览器解一次、取 `cf_clearance`,之后交还 reqwest 走普通快请求;**不是每个请求都走浏览器**(那样慢)。`cf_clearance` 管 30min+,一次求解覆盖整段会话的多次搜索。

```
ReqwestFetcher.fetch(req)
  └─ 响应命中挑战(D7)? ──否──► 正常返回
          │是
          ▼
  BrowserFetcher.solve(req.url)      # 升级:解挑战
    探测系统浏览器 ──无──► 降级(D8)
    headful 启动(持久 profile + --remote-debugging-port=0)
    导航 req.url,轮询 cf_clearance(协助式,D5)
    CDP Network.getAllCookies 读明文 cf_clearance(+其它 cf cookie)
    CDP /json/version 读真实 UA
  └─ 注入 reqwest cookie store + 记下真实 UA(D6)
  └─ 用 cf_clearance + 真实 UA 重发原请求 ──► 200 内容
```

实现上用一个组合 `Fetcher`(`EscalatingFetcher { reqwest, browser }`)包住二者,引擎只见 `trait Fetcher`,无感知。

### D4:headful + 持久化 profile(不用 headless、不用一次性目录)

- 实测 headless 没解开、headful 解开 ⇒ **必须 headful**。窗口可先后台/小窗,需要时(D5)再前置。
- **持久 profile**(`~/.novel/browser-profile/`)积累 cookie/历史/信誉,降低被升级为勾选框的概率,并跨会话缓存 `cf_clearance`。一次性目录每次都像新访客,最易触发勾选框。
- `--remote-debugging-port=0`(随机口)避免端口冲突;实测带调试口**不破坏**解挑战,且我们靠它读 cookie。

### D5:协助式解挑战状态机(绝不模拟点击)

```
启动并导航后,轮询 cf_clearance:
  ├─ 宽限期内(~3–5s)出现 → 静默成功,关窗,用户基本无感
  ├─ 超宽限期未出现 → CDP Runtime.evaluate 检测 Turnstile 勾选框是否可见
  │     ├─ 可见 → 窗口 bringToFront + TUI 提示「请在弹出的浏览器里点一下『确认您是真人』」
  │     │         继续轮询,直到 cf_clearance 出现或总超时
  │     └─ 不可见 → 继续等(非交互可能就是慢),直到总超时
  └─ 总超时(~60s)仍无 → 报失败,降级(D8)
```

- **不**用 CDP 合成点击 Turnstile(会被检测、循环);只让真人点。
- TUI 提示通过现有 WarningModal/事件机制实现;窗口前置由 CDP(`Page.bringToFront` / 窗口 bounds)完成。

### D6:UA 绑定——BrowserFetcher 必须回传真实 UA

`cf_clearance` 绑签发它的 UA(实测:写死 `Chrome/124` 会导致挑战拒发)。⇒ `BrowserFetcher` 解完后把浏览器真实 UA 一并交出,`EscalatingFetcher`/`ReqwestFetcher` 后续请求 MUST 用该 UA 覆盖书源里配置的 UA。

### D7:挑战检测信号

判定一次响应是「挑战页」而非内容:① 响应头 `cf-mitigated: challenge`(最干净,机器可读);② 403/503 + body 含 `_cf_chl_opt` / `/cdn-cgi/challenge-platform/` / `<title>Just a moment` 等特征。命中即触发升级(在线时)或在 `diagnose` 中标为精确诊断(离线/无浏览器时)。

### D8:降级链 + search 作为可选能力

```
撞挑战
 ├─ 有系统浏览器 → D3/D5 自动/协助解
 ├─ 无浏览器 / 解不开 / 用户取消
 │     ├─ 书源/会话有手贴 cf_clearance(http.cookies)→ 用它
 │     └─ 否则 → 该能力(如 search)报「需浏览器辅助,当前不可用」,引导改用浏览(explore)
```

- 书源可声明能力可选性(如 `search` 依赖 browser)。`diagnose`/`doctor` 把「被挑战、需浏览器」如实呈现为一个明确状态,而非笼统 ✗。
- 是否真的升级到浏览器,还受 D12 的两级配置(书源声明 ∧ 用户授权)门控;用户未授权等同「无浏览器」,走本降级链。

### D9:依赖与平台、feature 门控

- 新增:`chromiumoxide`(纯 Rust)、浏览器探测(`which` 或手写路径表)、可能少量序列化。**零新增 C 依赖**,浏览器是用户既有安装、不链接进二进制。
- 以 `browser` feature 门控 `BrowserFetcher` 与 `chromiumoxide`;默认构建可不含,按需开启,保持默认二进制精简。
- 平台差异(浏览器探测、窗口前置)集中在适配层。

### D10:取页编排落点(设计模式)

- **装饰器/责任链**:`EscalatingFetcher` 包 `ReqwestFetcher` + `BrowserFetcher`,对引擎仍是单一 `trait Fetcher`(端口不变)。
- **策略**:浏览器探测按平台选择路径表/探测法。
- **状态机**:D5 协助式解挑战。

### D11:生命周期与安全

- 子进程**随 Drop 必杀**(kill-on-exit guard),避免残留 Chrome;app 崩溃也不留僵尸。
- 单实例:同一 profile 同时只跑一个浏览器(profile 锁);并发求解串行化或共享同一会话。
- 所有等待有超时上限(D5);绝不无限轮询。
- 只导航书源声明的目标域;不注入/执行站点外脚本。

### D12:取页模式两级配置与用户授权

「是否动用浏览器」由两级共同决定,**取交集**:

- **书源级 `http.fetcher`**:`auto`(默认——平时 reqwest,撞挑战才升级浏览器)| `reqwest`(永不开浏览器,撞挑战即降级)| `browser`(整站强制浏览器:首请求即被挑战、或整页 JS 渲染的站)。并支持**能力级**可选性(如仅 `search` 依赖浏览器)。
- **app/用户级授权**:全局开关「是否允许浏览器辅助验证」。启动用户的浏览器涉及隐私与打扰,**不能仅由书源决定**;建议默认「首次询问、记住选择(本次 / 总是 / 拒绝)」,持久化到 app 配置(`~/.novel`)。
- **有效行为 = 书源声明需要 ∧ 用户允许**。用户未授权时,无视书源的 `browser`/`auto` 升级请求,一律走降级链(D8)。
- 替代方案:仅书源级——否决(越过用户同意擅自开浏览器不可接受);仅 app 级——否决(无法表达「某站整站需浏览器」)。

## Risks / Trade-offs

- [自适应挑战可能要真人点击] → 接受;协助式 UX(D5)+ 持久 profile 养号(D4)把频率压到「偶发一次/会话」;文档讲清。
- [纯 Safari/Firefox 用户无 Chromium 系] → 降级链(D8):手贴 cookie 或该能力不可用 + 引导浏览。
- [headless 在别的机器上也许能过,本设计强制 headful] → 故意保守:headful 成功率最高,且唯一能容纳真人点击;不为省一闪窗口赌检测。
- [cf_clearance 过期 / 站点改规则] → 过期或再撞挑战即重新求解(D3);`diagnose` 持续反映真实状态。
- [TUI→GUI→TUI 上下文切换突兀] → 仅在需点击时发生且罕见;窗口默认后台,仅必要时前置。
- [chromiumoxide/CDP 跨平台子进程脆弱性] → 生命周期守卫(D11)+ 超时;失败即降级,不影响读取链路。
- [某些站点 cf_clearance 也绑 TLS 指纹(bilixs 实测不绑)] → 那种站点退化为「全程走浏览器取页」或标记不支持;本期不为其引入 wreq。

## Migration Plan

- 纯增量:`browser` feature 默认可关;关闭时行为与现状完全一致(撞挑战 → 据实诊断/降级)。
- 分步:① 挑战检测 + 诊断(无浏览器也立即受益,先落);② `BrowserFetcher` + 探测 + cookie 烤箱(非交互路径);③ 协助式交互(Turnstile 提示 + 窗口前置);④ app 接线 + profile + TUI 提示;⑤ bilixs 书源 search 选择器修正(`.module-search-item`)与能力标注。
- 回滚:关闭 `browser` feature 或书源置 `http.fetcher: reqwest`。

## Open Questions

- 内置/下载 Chromium 是否作为「无系统浏览器」时的可选降级(复刻 TTS 模型按需下载)?本期不做,记此备议。
- `http.fetcher` 取值与「能力可选性」声明的最终 schema 形态(放 `http` 下还是各能力块下),在 specs 固化。
- 并发多次搜索时共享一个浏览器会话的策略(串行 vs 单例长驻 vs 用完即关)。
- 协助式点击的 TUI 呈现细节(模态文案、超时/取消交互)留实现期定。
