## ADDED Requirements

### Requirement: 脚本登录(loginUrl + login())

书源 JSON SHALL 支持 `loginUrl` 字段,其值可为普通 URL,或以 `@js:` / `<js>...</js>` 包裹的登录脚本。当为脚本时,引擎 MUST 约定其中导出一个全局函数 `login()`,并在用户触发登录时拼接固定壳调用它(`if(typeof login=='function') login.apply(this)`)。脚本内部 MUST 能通过 host 桥 `source.getLoginInfo()` 取凭据、`net.connect/post` 发请求、`source.putLoginHeader(...)` 写回登录态。

#### Scenario: 脚本登录写回 loginHeader
- **WHEN** 用户触发登录,`login()` 脚本发请求拿到 token 后调用 `source.putLoginHeader(JSON.stringify({Authorization:'Bearer '+token}))`
- **THEN** 该 loginHeader 被持久化,后续请求自动携带

### Requirement: 声明式登录表单(loginUi)

书源 JSON SHALL 支持 `loginUi` 字段(RowUi JSON 数组,每项含 `name`/`type`,type ∈ {text, password, select, toggle})。引擎 MUST 在 TUI 中渲染对应输入控件(password 掩码、select 列表、toggle 布尔),收集到的值经加密存为 `loginInfo` 供 `login()` 读取。

#### Scenario: 渲染密码输入
- **WHEN** loginUi 含 `{name:'密码', type:'password'}`,用户在 TUI 登录表单输入
- **THEN** 输入以掩码显示,提交后值加密存入 loginInfo

### Requirement: headful 浏览器登录

当书源声明需要浏览器登录(仅 `loginUrl` 为普通 URL、或脚本调用 `startBrowserAwait`)时,引擎 SHALL 起 headful 浏览器打开登录页,让用户在真实官网手动完成登录/验证码/2FA。登录成功后引擎 MUST 同时尝试提取 cookie(`get_cookies`,含 HttpOnly)与 localStorage 中的 token(`evaluate`),并写入登录态。成功判定 SHALL 支持「目标 cookie/localStorage 键出现」或「用户在 TUI 确认完成」。

#### Scenario: 提取 cookie 登录态
- **WHEN** 用户在 headful 浏览器登录后,目标 `sessionid` cookie 出现
- **THEN** 引擎将其写入 cookie 库并标记登录成功

#### Scenario: 提取 localStorage 中的 JWT
- **WHEN** 站点把 JWT 存于 `localStorage.access_token`,用户登录完成
- **THEN** 引擎通过 `evaluate` 读出该 token 并存为 loginHeader(`Authorization: Bearer`)

### Requirement: 登录态统一注入(loginHeader 覆盖 cookie 与 JWT)

引擎构造每个网络请求时,SHALL 在请求头合并的最后并入该书源的 `loginHeader`(任意 header map),默认开启。`putLoginHeader` 写入时,若 header map 含 `Cookie`/`cookie` 字段,引擎 MUST 额外将其同步进 cookie 库。由此 `Authorization: Bearer <jwt>`、自定义 token 头、`Cookie` 三种登录态 MUST 走同一条注入路径,无需针对 JWT 的专门逻辑。

#### Scenario: JWT 每请求自动携带
- **WHEN** loginHeader 为 `{Authorization:'Bearer xxx'}`,发起任意书源请求
- **THEN** 该请求头自动带上 `Authorization: Bearer xxx`

#### Scenario: loginHeader 中的 Cookie 同步到 cookie 库
- **WHEN** `putLoginHeader('{"Cookie":"sid=1"}')`
- **THEN** `sid=1` 既存入 loginHeader,也被同步进 cookie 库

### Requirement: 凭据加密存储(loginInfo)

用户填写的账号/密码等敏感凭据 SHALL 以对称加密(AES,复用现有 transform)落盘,与明文 `loginHeader` 分桶存储。`source.getLoginInfo()` MUST 返回解密后的明文供登录脚本使用。

#### Scenario: 凭据加密落盘
- **WHEN** 用户在 loginUi 提交账号密码
- **THEN** 落盘文件中凭据为密文(非明文),`source.getLoginInfo()` 能解密读回

### Requirement: cookie 持久化与回灌

cookie 库 SHALL 按二级域名(注册域)归并存储并落盘,session cookie 与 persistent cookie 分离(session 仅内存、重启失效)。书源声明 `enabledCookieJar` 时,引擎 MUST 在每次响应后将 `Set-Cookie` 自动回灌入库;每次请求前 MUST 合并库中 cookie 进 `Cookie` 头。

#### Scenario: 子域共享登录 cookie
- **WHEN** 登录在 `www.site.com` 拿到 cookie,随后请求 `api.site.com`
- **THEN** 因按二级域名 `site.com` 归并,api 子域请求自动带上该 cookie

#### Scenario: cookieJar 回灌
- **WHEN** `enabledCookieJar` 开启,某响应返回 `Set-Cookie`
- **THEN** 该 cookie 被存入库,后续请求自动携带

### Requirement: 登录态过期校验与重登(loginCheckJs)

书源 JSON SHALL 支持 `loginCheckJs` 字段,在每个网络方法(搜索/详情/目录/正文)的响应后执行,响应对象注入为 `result`。该脚本 MUST 能判断登录是否失效;失效时可触发重登并返回修正后的响应。第一版若不支持自动重登,失效 MUST 以明确错误提示用户重新登录。

#### Scenario: 检测掉登录
- **WHEN** 响应体表明未登录,`loginCheckJs` 判定失效
- **THEN** 引擎提示用户重新登录(或在支持时自动重登重发)

### Requirement: 登录入口与时机

引擎 SHALL 提供 `hasLogin` 判定(`loginUrl` 或 `loginUi` 非空即为需登录),据此在 TUI 暴露「书源登录」入口。登录 MUST 是按需手动触发(用户主动登录,或请求失败/loginCheckJs 判失效时提示),而非书源加载时自动执行。登录产物(loginHeader/loginInfo/cookie)MUST 按书源 url_md5 持久化、跨会话复用。

#### Scenario: 仅对需登录书源显示入口
- **WHEN** 书源声明了 loginUrl 或 loginUi
- **THEN** TUI 显示该书源的「登录」入口;未声明的书源不显示
