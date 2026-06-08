## Context

`parse-book-source` 当前的运行时由四块组成:reqwest 取页(GET/POST/headers/cookie_store)、boa JS 沙箱(`js.rs`,刻意「只注入 result/baseUrl/vars/crypto,无网络、无状态,攻击面极小」)、chromiumoxide headful(`browser.rs`,仅用于解 Cloudflare 挑战取 `cf_clearance`)、以及 AES/hash/cookie 等纯函数 transform。

这套架构读「无需登录的网络小说」很干净,但**无法处理需要登录的站点**:登录态(cookie/JWT)拿不到也注入不进、跨请求无法传值、多步 API 编排(书架/历史)无从写起。

本 change 基于对 **Legado(阅读)** 源码的实证调研(`BaseSource.kt` / `JsExtensions.kt` / `AnalyzeUrl.kt` / `CookieStore.kt` / `SourceVerificationHelp.kt`)。Legado 的核心模型是:

```
  interface BaseSource : JsExtensions   ← source 与 java 是同一个有状态对象
  evalJS 注入: java/source(网络+状态)、cookie(CookieStore)、cache(CacheManager)
  所有状态以 getKey()(=bookSourceUrl)做命名空间隔离
  登录态 = loginHeader(任意 header map),每请求自动 merge;JWT 与 Cookie 统一
  多步 = net.ajax(复用 URL 管线) + source.put/get + 结构化 vars 捕获 + 变量作用域链
```

用户已在 explore 阶段选定 **方案 A:JS 全开 + 注入有状态 host 对象**(跟随 Legado),放弃更保守的方案 B(纯声明式、JS 不开网络)与方案 C(feature-gate 折中)。本设计据此展开。

## Goals / Non-Goals

**Goals:**
- 给 boa 沙箱注入两个语义对象 `source`(状态/登录)与 `net`(网络/cookie/浏览器)(`crypto` 沿用现有),共同暴露:网络(`net.ajax`/`connect`/`get`/`post`/`ajaxAll`)、跨请求状态(`source.put`/`get`/`getVariable`/`putVariable`)、cookie(`net.getCookie`)、登录态(`source.putLoginHeader`/`getLoginHeader`/`getLoginInfo`)、浏览器(`net.startBrowserAwait`)。
- 三种登录范式:`loginUrl`(`@js:` 脚本,约定 `login()` 入口)、`loginUi`(声明式表单)、headful 浏览器登录(复用 chromiumoxide)。
- `loginHeader` 统一登录态注入:每个 reqwest 请求自动 merge,**JWT(`Authorization: Bearer`)与 Cookie 走同一路径**。
- per-source 持久状态:`~/.novel/source-state/{url_md5}.json`(kv / variable / loginHeader / loginInfo 加密 / cookie)。
- 多步请求编排:JS `ajax` 复用 URL 管线 + 结构化 `vars` 命名捕获 + 变量作用域(章节→书籍→书源)级联。
- cookie 按二级域名持久化 + `enabledCookieJar` 回灌。

**Non-Goals:**
- 不内置任何站点专属登录逻辑(交给书源 JS + 用户在官网手动登录)。
- 不内置 OAuth/2FA/滑块求解(headful 浏览器让用户自己过)。
- 第一版不做 `loginUi` 按钮的 `action` JS(只做静态表单收集)。
- 不追求与 Legado JS API 全量对齐(只挑登录/多步核心子集)。
- 不在 host 桥里开放文件系统/进程等高危能力(只开网络 + 受控的 per-source 状态)。

## Decisions

### D1. 打破纯沙箱、注入有状态 host(方案 A)
JS 全开是登录与多步编排的统一基础——脚本登录(`login()`)、多步(`net.ajax`)、状态(`source.put`)全建立其上。
- **备选**:B(结构化 `vars` 声明式 + headful,JS 不开网络)——更安全但写不了复杂多步;C(feature-gate)——安全可调但心智复杂。
- **理由**:用户选 A;host 桥一次到位、复用度最高。
- **攻击面缓解**:host 桥在 `js` feature 下;书源**首次使用网络/状态能力需用户授权**(对应 Legado 的 `enableDangerousApi`);host API 白名单(只网络 + per-source 状态 + cookie + 浏览器,**不开** fs/exec);凭据加密。

### D2. 同步阻塞桥接 reqwest 进 boa
boa 单线程同步,Legado 用 `runBlocking` 把 suspend 网络桥成同步。
- **选**:`net.ajax` 的 native function 内对 reqwest future 做 `block_on`。
- **风险点**:不能在已持有的 tokio worker 线程内 `block_on`(死锁)。用专用 runtime handle 或把 JS 求值整体放 `spawn_blocking`,在其中用独立 current-thread runtime 驱动网络。
- **备选**:预取数据注入——丧失多步灵活性,弃。

### D3. 注入 `source` + `net` 两个语义对象(不沿用 Legado 的 `java`)
一个 Rust `SourceHost` struct,在 JS 侧暴露为两个职责分明的对象:`source`(状态/登录:`put`/`get`/`getVariable`/`putLoginHeader`/`getLoginInfo`/`login`)与 `net`(网络/cookie/浏览器:`ajax`/`connect`/`get`/`post`/`getCookie`/`startBrowserAwait`);加解密沿用现有 `crypto`。底层同一实例(共享 SourceState/client/cookie 库),JS 侧分对象只为语义清晰。
- **理由**:TRNovel 是 Rust 项目,Legado 的 `java`(= JsExtensions 的 Java 实现)命名无语义且误导;`source`/`net` 直观表达「书源状态」与「网络」。

### D4. 登录态 = `loginHeader`(任意 header map),JWT 与 Cookie 统一
`putLoginHeader(json)` 把 header map 存进 source-state;若其中含 `Cookie` 字段则**额外同步进 cookie 库**。reqwest 构造每个请求时在 header 合并的最后 merge `loginHeader`(默认开启)。
- **理由**:Legado 实证——一个 header map 覆盖 `Authorization: Bearer <jwt>`、自定义 token 头、`Cookie` 三种登录态,无需任何 JWT 专属逻辑。这正是 explore 中用户「万一是 JWT 不是 cookie」问题的标准答案。

### D5. per-source 状态统一文件
`~/.novel/source-state/{url_md5}.json` 一个文件装下 `kv`(put/get)、`variable`(用户配置槽)、`login_header`(明文 JSON)、`login_info`(AES 密文)、`cookies`(按二级域名),可选 `expire_at` 实现 TTL。
- **理由**:复用现有 serde_json + Drop-autosave 模式,与 `network/{url_md5}.json` 命名一致;无需引入 SQLite(Legado 用 Room,TRNovel 更轻)。

### D6. headful 登录复用 chromiumoxide
`browser.rs` 从「解 CF」升级为通用「起浏览器让用户登录/过验证」:打开 `loginUrl` → 用户手动登录 → `get_cookies`(cookie 含 HttpOnly)+ `page.evaluate("localStorage")`(取 JWT)→ 成功判定(目标 cookie/localStorage 出现,或用户在 TUI 确认)→ 写入 source-state。
- LockSupport park/unpark 用 tokio `Notify`/`oneshot` 替代。
- `startBrowserAwait` 可返回登录后页面 HTML(`refetchAfterSuccess`:用拿到的 cookie 走 reqwest 重抓一遍得干净响应)。

### D7. 多步请求编排两条腿
- (a) **JS 内** `net.ajax`/`connect` 复用完整 URL 解析管线(url 可再带 `,{option}`、内嵌 `<js>`/`{{}}`、自动带 loginHeader+cookie)——递归式 JS↔URL-DSL。
- (b) **结构化声明式**(对齐 TRNovel 的 ai-friendly、无字符串 DSL 原则,**不引入 Legado 的 `@put:/@get:` 字符串 DSL**):用**结构化命名捕获** `vars: { name: <Rule> }`——在某请求响应后对每个 `Rule` 求值、存入作用域变量;后续请求的 URL/header/body 用现有 `{{name}}` 模板插值引用。这正好接通已定义未接线的 `Request.vars`(本就是 `HashMap<String, Rule>` 的结构化捕获)。捕获 MUST 先于引用它的步骤求值;变量作用域章节→书籍→书源级联回退。AI(booksource-generator)生成的是结构化 JSON 字段,而非嵌套字符串——这是相对 Legado 的关键设计差异。

### D8. cookie 库升级
按二级域名(publicsuffix 取注册域)归并落盘;session(非 persistent,内存,重启失效)与 persistent 分离;`enabledCookieJar` 时自动回灌响应 `Set-Cookie`;请求前合并库 cookie 进 `Cookie` 头(临时 urlOption cookie 优先)。`cf_clearance`(browser-fetcher)、headful 登录 cookie、`Set-Cookie` 三路汇入同一库。

### D9. 凭据加密
`login_info`(账号/密码等)用 AES(复用现有 transform)落盘,密钥派生自机器标识(`machine-uid` 或用户主目录派生盐),与明文 `login_header` 分桶。对应 Legado 用 androidId 前 16 字节做 AES key 的设计。

### D10. 登录态过期处理
`loginCheckJs` 在每个网络方法的响应后执行(注入 `result`=响应),JS 判断是否掉登录、可 `source.login()` 后重发、返回最终响应。第一版可简化为:返回 false → 抛「需要重新登录」错误,弹给用户。

### D11. `loginUi` TUI 渲染
RowUi DSL(`text`/`password`/`select`/`toggle`)用 ratatui-kit 渲染:password 掩码、select 列表、toggle 布尔。复用现有 SearchInput 思路。

## Risks / Trade-offs

- **JS 全开 → 攻击面变大,书源能联网/读写本地状态** → host API 白名单(只网络/状态/cookie/浏览器,**不开 fs/exec**)+ `js` feature 门控 + 书源首次用网络能力需用户授权 + 凭据加密。
- **boa `block_on` reqwest 在 async 上下文重入死锁** → JS 求值整体走 `spawn_blocking` + 独立 current-thread runtime,或注入专用 runtime handle;明确禁止在 tokio worker 内 block。
- **登录态/凭据落盘泄露** → `login_info` AES 加密;文件权限收紧;UI 提示这是敏感凭据。
- **登录态过期 / JWT refresh** → `loginCheckJs` 重登;refresh token 这类多步由书源 JS 自行编排(host 桥已提供 ajax+put/get)。
- **headful 需桌面 + 系统 Chrome** → 无浏览器时降级(同 browser-fetcher 既有策略);CI/沙箱跑不了浏览器,登录/多步端到端只能手动联调。
- **boa 无 Rhino 的 prototype 共享,jsLib 难复刻** → 用同一 Realm 或「库源码前置拼接」模拟;jsLib 列为后续。
- **测试难(需真账号+真浏览器)** → 单测覆盖 host 桥纯逻辑(状态存取、loginHeader merge、cookie 二级域名归并、`vars` 捕获时序);登录/多步靠 `trn doctor` + 手动。

## Migration Plan

- **向后兼容**:host 桥在 `js` feature 下增量;书源不声明 `loginUrl`、不调 `net.ajax` 则行为与现状完全一致。现有书源零影响。
- **分阶段**(详见 tasks.md):
  - **P1**:host 桥骨架(注入 source/net + 网络 ajax/connect + put/get + cookie 桥)+ source-state 持久化 + loginHeader 注入 + headful 登录(`login()` / `startBrowserAwait`)。← 价值最大、覆盖「登录读全本」。
  - **P2**:cookie 库升级(二级域名归并 + cookieJar 回灌)+ 结构化 `vars` 接通 + `Request.vars` 前置请求 + 变量作用域。
  - **P3**:`loginCheckJs` 响应期重登 + `loginUi` TUI 表单 + `concurrentRate` 限速 + jsLib 共享作用域。
- **回滚**:host 桥是新增注入,关闭 feature 即回到纯沙箱;source-state 文件独立,删除不影响既有缓存。

## Open Questions

**已在 apply(P0)阶段拍板:**
- host 桥门控 → **新增 `js-host` feature**(与纯 transform 的 `js` 分开;默认发行版可不带网络能力,书源需要才开)。
- boa 网络桥 → **整段 JS 求值走 `spawn_blocking` + 独立 current-thread runtime** 驱动 reqwest;绝不在主 tokio worker 内 block(最不易死锁、隔离最干净)。
- 凭据加密密钥 → **`machine-uid` 绑机器**派生 AES key(凭据拷到别的设备不可解,类比 Legado androidId)。
- `cf_clearance` → **并入 source-state 统一 cookie 库**(CF/登录/Set-Cookie 三路 cookie 汇于一处,单一注入路径)。

**仍待定(实现中按需决定):**
- `loginUi` 第一版做到多全(text/password 够用,还是要 select/toggle/button-action)?
- 书源「首次使用网络能力需授权」的授权粒度与持久化(per-source once/always)?
