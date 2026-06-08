## Why

TRNovel 目前只能读「无需登录的网络小说」。一旦碰到**需要登录的站点**——番茄小说会员读全本、JWT 鉴权的现代书站、需要书架/阅读历史的个人中心——就读不了:`parse-book-source` 的 boa JS 沙箱被刻意做成「无网络、无状态」(攻击面极小),导致**登录态(cookie/JWT)无法获取与注入、跨请求无法传值、多步 API 编排无从实现**。

Legado(阅读)的成熟做法是「给书源 JS 注入一个能联网、能读写持久状态的 host 对象(`source`+`net`)」,登录、cookie/JWT、多步请求全建立在它之上。经调研其 `BaseSource.kt`/`JsExtensions.kt`/`AnalyzeUrl.kt`/`CookieStore.kt` 源码后,本 change 采用该路线(**方案 A:JS 全开 + 有状态 host 桥**),为书源引擎补齐认证与多步请求能力。

## What Changes

- **给 boa 沙箱注入有状态 host 对象**(`source`+`net` 指向同一实例),暴露:网络(`ajax`/`connect`/`get`/`post`/`ajaxAll`)、跨请求状态(`put`/`get`/`getVariable`)、cookie(`getCookie`)、登录态(`putLoginHeader`/`getLoginHeader`/`getLoginInfo`)、浏览器(`startBrowserAwait`)。**这是对现有「纯沙箱无网络无状态」安全模型的重大调整**,以 feature 门控 + 书源授权约束攻击面。
- **登录态注入(统一 JWT + Cookie)**:reqwest 构造每个请求时自动 merge `loginHeader`(任意 header map);若其中含 `Cookie` 字段则同步进 cookie 库。**JWT(`Authorization: Bearer`)与 Cookie 走同一条注入路径**——这是 Legado 的关键设计,一招覆盖两种登录态。
- **三种登录范式**:`loginUrl`(`@js:` 登录脚本,约定 `login()` 入口函数)+ `loginUi`(声明式表单 RowUi,TUI 渲染)+ **headful 浏览器登录**(复用 chromiumoxide,等价 Legado `startBrowserAwait`,让用户在官网手动登录/过验证码,引擎抓 cookie / `evaluate` 取 localStorage 里的 JWT)。
- **per-source 持久状态**:`~/.novel/source-state/{url_md5}.json` 统一存 kv / variable / loginHeader / loginInfo(加密) / cookie,按书源 url 的 md5 命名空间隔离,复用现有 serde_json + Drop-autosave 模式。
- **多步请求编排**:JS 内 `ajax`/`connect` 复用完整 URL 解析管线(url 可再带 option、自动带 loginHeader+cookie);结构化 `vars` 命名捕获跨请求传值;`source.put/get` 跨请求 KV;变量作用域(章节→书籍→书源)级联回退。
- **cookie 持久化升级**:按二级域名归并落盘、session/persistent 分离、`enabledCookieJar` 自动回灌响应 `Set-Cookie`。
- **登录态过期处理**:`loginCheckJs` 在响应期校验登录是否失效,失效时重登重发(第一版可简化为提示用户重新登录)。

## Capabilities

### New Capabilities

- `js-host-bridge`: 给 boa JS 沙箱注入有状态 host 对象(`source`+`net`)——网络(reqwest 桥)、跨请求 KV/变量、cookie 读写、headful 浏览器(chromiumoxide 桥);含 per-source 持久状态存储与变量作用域。是登录与多步编排的运行时基础设施。
- `source-auth`: 书源登录与登录态管理——`loginUrl`(login() 脚本)/ `loginUi`(表单)/ headful 浏览器三种登录范式;`loginHeader` 统一注入 cookie 与 JWT;凭据(loginInfo)加密存储;`loginCheckJs` 过期校验与重登。

### Modified Capabilities

(无:`openspec/specs/` 下暂无已落盘的 capability spec。本 change 对 js-rule-engine「纯沙箱」安全模型与 browser-fetcher「仅解 CF」用途的扩展,在上述两个新 spec 中正式描述。)

## Impact

- **`crates/parse-book-source`**:
  - `js.rs`(boa 沙箱)— 从「只注入 result/baseUrl/vars/crypto」扩展为注入有状态 `source`+`net` host 对象 + native function 回调进 Rust。**本 change 最大的架构改动,直接调整 js-rule-engine 的安全模型**。
  - `fetch.rs`(reqwest)— 请求头合并顺序加入 loginHeader 层;cookie 库按二级域名归并 + 回灌。
  - `source.rs`— 书源 JSON 新增 `loginUrl`/`loginUi`/`loginCheckJs`/`enabledCookieJar`/`concurrentRate` 字段;接通已定义未接线的 `Request.vars`。
  - `browser.rs`(chromiumoxide)— 从「只解 CF 取 cf_clearance」扩展为通用「起浏览器让用户登录/过验证」(`get_cookies` + `evaluate` 取 localStorage JWT)。
  - `eval.rs` + 新增 state 模块 — `vars` 捕获求值时序、前置请求;per-source 状态持久化。
- **root crate `trnovel`**:TUI 新增「书源登录」入口/页面(`hasLogin` 判断、loginUi 表单渲染、触发 headful 登录);`~/.novel/source-state/` 缓存目录。
- **安全**:JS 获得网络与本地状态读写能力 → 需 feature 门控 + 书源授权(对应 Legado 的 `enableDangerousApi`);凭据加密;登录态文件权限。
- **schema/文档**:`book-source.schema.json` 新增字段;`booksource-generator` skill 补登录/多步编排章节。
- **依赖**:boa host 桥需在 JS 调用边界用 `block_on` 同步调用 reqwest;chromiumoxide(已有 `browser` feature)。
