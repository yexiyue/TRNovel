## ADDED Requirements

### Requirement: 识别反爬挑战响应

系统 SHALL 把命中以下任一特征的响应识别为「挑战页」而非内容,并 MUST NOT 将其当作成功结果交给规则求值:① 响应头 `cf-mitigated: challenge`;② HTTP 403/503 且 body 含 `_cf_chl_opt`、`/cdn-cgi/challenge-platform/` 或 `<title>Just a moment` 等挑战特征。

#### Scenario: 通过 cf-mitigated 头识别

- **WHEN** 取页响应头含 `cf-mitigated: challenge`
- **THEN** 系统判定为挑战页,不把 body 当内容,并标记需升级取页或降级

#### Scenario: 通过挑战页特征识别

- **WHEN** 响应为 403 且 body 含 `_cf_chl_opt` 与 `/cdn-cgi/challenge-platform/`
- **THEN** 系统判定为挑战页

#### Scenario: 正常内容不误判

- **WHEN** 响应为 200 且为真实书页(无上述特征)
- **THEN** 系统正常交给规则求值,不触发任何升级/降级

### Requirement: 精确诊断而非笼统失败

`diagnose` / `doctor` SHALL 把「被反爬挑战拦截」呈现为一个独立、精确的状态(说明被挑战、需浏览器辅助或更换入口),而非笼统的失败项。

#### Scenario: doctor 报告被挑战

- **WHEN** 对某书源跑 `doctor`,其搜索端点返回挑战页
- **THEN** 报告中该项显示「被 Cloudflare 托管挑战,需浏览器辅助/或改用浏览」,而非仅一个 ✗

### Requirement: 能力可选性与降级引导

书源 SHALL 可声明某能力(如 `search`)依赖浏览器辅助;当该能力因挑战不可用且无可用浏览器时,系统 MUST 据实报告不可用并引导改用浏览(explore),而非静默失败或假装成功。

#### Scenario: 搜索不可用时引导浏览

- **WHEN** 书源声明 `search` 依赖浏览器,且当前无可用浏览器解挑战
- **THEN** 系统报告搜索暂不可用,并提示可改用分类浏览发现书籍

### Requirement: 在线撞挑战触发取页升级

当启用浏览器取页(`browser` feature 且书源 `http.fetcher` 允许)且在线请求撞到挑战时,系统 MUST 触发向浏览器取页的升级(见 `book-source-browser-fetcher`),而非直接失败。

#### Scenario: 撞挑战触发升级

- **WHEN** `ReqwestFetcher` 的响应被识别为挑战页,且浏览器取页可用且已获用户授权
- **THEN** 系统升级到 `BrowserFetcher` 求解挑战,然后重试原请求

### Requirement: 取页模式两级可配置且受用户授权门控

是否动用浏览器 SHALL 由两级共同决定、取交集:① **书源级** `http.fetcher`,取值 `auto`(默认:平时 reqwest,撞挑战才升级)、`reqwest`(永不开浏览器)、`browser`(整站强制浏览器),并可在能力级(如 `search`)声明依赖浏览器;② **app/用户级**全局授权开关。系统 MUST NOT 在用户未授权时启动浏览器;用户未授权时,撞挑战等同「无浏览器」而走降级链。

#### Scenario: 书源声明 reqwest 则永不开浏览器

- **WHEN** 书源 `http.fetcher` 为 `reqwest`,且某端点被挑战
- **THEN** 系统不启动浏览器,直接走降级链(报不可用/引导浏览)

#### Scenario: 用户未授权则不开浏览器

- **WHEN** 书源声明需要浏览器(`auto` 撞挑战或 `browser`),但用户未授权浏览器辅助
- **THEN** 系统不启动浏览器,走降级链,并据实告知「需浏览器辅助但未授权」

#### Scenario: 书源需要且用户授权则升级

- **WHEN** 书源 `http.fetcher` 允许且用户已授权,某端点被挑战
- **THEN** 系统升级到 `BrowserFetcher` 求解
