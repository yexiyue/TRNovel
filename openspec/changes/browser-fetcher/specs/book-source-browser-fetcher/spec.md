## ADDED Requirements

### Requirement: 探测系统已装浏览器

`BrowserFetcher` SHALL 探测用户系统已装的 Chromium 系浏览器(Chrome / Edge / Brave / Chromium / Vivaldi),按平台查找:macOS 扫 `/Applications`,Windows 查注册表 / `%ProgramFiles%` / `%LOCALAPPDATA%`,Linux 用 `which`。找不到任何 Chromium 系浏览器时 MUST 进入降级(不得崩溃)。

#### Scenario: macOS 找到 Chrome

- **WHEN** 系统装有 Google Chrome,平台为 macOS
- **THEN** `BrowserFetcher` 定位到 Chrome 可执行文件并可用其取页

#### Scenario: Windows 回退到 Edge

- **WHEN** 平台为 Windows 且未装 Chrome
- **THEN** `BrowserFetcher` 使用系统自带的 Microsoft Edge

#### Scenario: 无 Chromium 系浏览器时降级

- **WHEN** 系统未装任何 Chromium 系浏览器
- **THEN** `BrowserFetcher` 不可用,系统转入降级链(手贴 cookie / 能力不可用),而非崩溃

### Requirement: headful 启动与持久化 profile

`BrowserFetcher` SHALL 以 **headful** 方式启动浏览器(非 headless),使用**持久化 profile** 目录 `~/.novel/browser-profile/` 与随机调试端口(`--remote-debugging-port=0`)。窗口 MAY 默认置于后台/最小化。

#### Scenario: 持久 profile 跨会话复用

- **WHEN** 上一次会话已在该 profile 取得有效 `cf_clearance`,本次再次请求同站
- **THEN** 复用 profile 中仍有效的 `cf_clearance`,无需重新解挑战

#### Scenario: 调试端口不冲突

- **WHEN** 启动浏览器用 `--remote-debugging-port=0`
- **THEN** 系统通过 CDP 端点发现实际端口并连接,不因固定端口冲突而失败

### Requirement: Cookie 烤箱式取页

`BrowserFetcher` SHALL 仅用浏览器**解挑战、签发 `cf_clearance`**:导航被挑战 URL → 解挑战 → 通过 CDP `Network.getAllCookies` 读取**明文** `cf_clearance`(及其它 cf cookie)→ 注入 reqwest 的 cookie store。解挑战后,后续取页 MUST 由 reqwest 普通请求完成,而非每个请求都走浏览器。

#### Scenario: 解挑战后由 reqwest 取页

- **WHEN** 浏览器解开挑战并签发 `cf_clearance`
- **THEN** 系统把 `cf_clearance` 注入 reqwest,后续对受保护端点的请求由 reqwest 直接发出并得到 200 内容

#### Scenario: clearance 有效期内不重复开浏览器

- **WHEN** 已有有效 `cf_clearance` 且会话内多次搜索
- **THEN** 仅首次开浏览器解挑战,其余搜索复用同一 `cf_clearance`

### Requirement: 协助式解挑战且绝不模拟点击

`BrowserFetcher` SHALL 轮询 `cf_clearance`:宽限期内出现则**静默成功并关窗**;超宽限期未出现且检测到 Turnstile 勾选框可见,则把窗口**提到最前**并通过 TUI 提示用户点击「确认您是真人」,随后继续轮询直至成功或总超时。系统 MUST NOT 通过 CDP 合成/模拟点击 Turnstile。所有等待 MUST 有总超时上限,超时则进入降级。

#### Scenario: 非交互路径静默成功

- **WHEN** 挑战为非交互型,宽限期内签发 `cf_clearance`
- **THEN** 系统静默关闭浏览器窗口,用户无需任何操作

#### Scenario: 交互式挑战提示用户点击

- **WHEN** 挑战升级为可见的 Turnstile 勾选框,宽限期内未自动解开
- **THEN** 系统把浏览器窗口提到最前并提示用户点一下「确认您是真人」,用户点击后取得 `cf_clearance`

#### Scenario: 不模拟点击

- **WHEN** 出现 Turnstile 勾选框
- **THEN** 系统不通过 CDP 合成点击,而是等待真人点击

#### Scenario: 总超时降级

- **WHEN** 到达总超时上限仍未取得 `cf_clearance`
- **THEN** 系统放弃求解并转入降级链,不无限等待

### Requirement: UA 绑定

由于 `cf_clearance` 绑定签发它的 User-Agent,`BrowserFetcher` 解挑战后 MUST 把浏览器的**真实 UA** 一并交出,系统后续对该站的 reqwest 请求 MUST 使用该真实 UA(覆盖书源中配置的 UA)。

#### Scenario: 后续请求使用浏览器真实 UA

- **WHEN** 浏览器(如 Chrome 146)解挑战签发 `cf_clearance`
- **THEN** reqwest 后续请求携带 `cf_clearance` 且 User-Agent 与该浏览器一致,从而被接受(返回 200)

### Requirement: 子进程生命周期安全

`BrowserFetcher` SHALL 保证其启动的浏览器进程随其作用域结束(或 app 退出)被关闭,不留僵尸进程;同一 profile 同时 MUST 只运行一个浏览器实例。

#### Scenario: 退出时关闭浏览器

- **WHEN** 取页完成或 app 退出
- **THEN** 由 `BrowserFetcher` 启动的浏览器进程被关闭

#### Scenario: 单 profile 单实例

- **WHEN** 已有一个浏览器实例占用该 profile,又发起新的求解
- **THEN** 系统复用/串行化到同一实例,不并发占用同一 profile
