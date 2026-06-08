## ADDED Requirements

### Requirement: 注入有状态 host 对象(source + net)

JS 求值时,引擎 SHALL 向 boa 沙箱注入两个职责分明的语义对象:`source`(书源状态/登录:`put`/`get`/`getVariable`/`putLoginHeader`/`getLoginInfo`/`login`)与 `net`(网络/cookie/浏览器:`ajax`/`connect`/`get`/`post`/`getCookie`/`startBrowserAwait`);加解密沿用现有 `crypto`。两者底层可共享同一 host 实例,均按书源 url_md5 命名空间。host 能力 MUST 仅为白名单(网络、per-source 状态、cookie、浏览器),且 MUST NOT 暴露文件系统、进程或任意主机命令能力。引擎 MUST NOT 沿用 Legado 的 `java` 命名(无 Java 语义、误导)。

#### Scenario: source 与 net 职责分明
- **WHEN** 书源 JS 用 `source.put('k','v')` 存状态、用 `net.ajax(url)` 发请求
- **THEN** 状态走 `source`、网络走 `net`,各司其职;不存在 `java` 对象

#### Scenario: 不暴露高危能力
- **WHEN** 书源 JS 尝试访问文件系统/进程相关 API
- **THEN** 该能力不存在(host 对象未注入此类方法)

### Requirement: JS 网络桥(ajax/connect)

host 对象 SHALL 提供 `net.ajax(url)`(返回响应体字符串)与 `net.connect(url, extraHeadersJson?)`(返回**结构化属性**对象 `{body, code, headers}`——优于 Legado 的 `body()/headers()` 方法形式,契合 ai-friendly),在规则求值期发起额外 HTTP 请求。该请求 MUST 复用引擎的取页管线并自动附加当前书源的 `loginHeader`(JWT/Cookie 同路径)。URL 再带 `,{option}` 选项与内嵌 `{{}}`/`<js>` 的递归解析见任务 4.3。网络调用失败 MUST 以可被 JS 捕获的方式返回(不使整段求值崩溃)。

#### Scenario: ajax 复用取页管线并带登录态
- **WHEN** 书源已登录(存在 loginHeader),JS 执行 `net.ajax('{{base}}/api/shelf')`
- **THEN** 该请求自动带上 loginHeader,返回响应体

#### Scenario: connect 可读响应头与状态码
- **WHEN** JS 执行 `net.connect(url)` 并读 `.code` / `.headers['set-cookie']`
- **THEN** 能读到 HTTP 状态码与响应头(用于解析 Set-Cookie / Location 等)

### Requirement: 跨请求状态存储(put/get)

host 对象 SHALL 提供 `source.put(key, value)`(写入并返回 value)与 `source.get(key)`(读取,缺失返回空串而非抛错)。状态 MUST 按书源 url 的 md5 做命名空间隔离,且 MUST 跨请求、跨会话持久(落盘)。

#### Scenario: 跨请求读回
- **WHEN** 登录脚本 `source.put('token', t)` 后,后续目录请求的 JS `source.get('token')`
- **THEN** 返回先前写入的 `t`

#### Scenario: 缺失键返回空串
- **WHEN** JS 读取从未写入的 `source.get('nope')`
- **THEN** 返回空字符串(不抛异常)

### Requirement: 变量作用域级联

规则变量 SHALL 支持「章节 → 书籍 → 书源」三级作用域:`put` 时按就近落点,`get` 时按章节→书籍→书源顺序级联回退取第一个非空值。书籍级变量 MUST 随该书的 per-book 快照持久化。

#### Scenario: 就近落点与级联回退
- **WHEN** 书源级存在 `k=A`,当前书籍级写入 `k=B`,在该书上下文 `get('k')`
- **THEN** 返回书籍级的 `B`(就近优先)

### Requirement: cookie 读写桥

host 对象 SHALL 提供 `net.getCookie(domain, key?)` 读取 cookie 库中指定域名(可选指定 cookie 名)的值,供登录脚本校验/读取登录 cookie。

#### Scenario: 读取登录 cookie
- **WHEN** headful 登录后 cookie 库已有 sessionid,JS 执行 `net.getCookie('fanqienovel.com','sessionid')`
- **THEN** 返回该 sessionid 值

### Requirement: headful 浏览器桥

host 对象 SHALL 提供 `net.startBrowserAwait(url, title?)`,以 headful 方式起系统浏览器(复用 chromiumoxide),阻塞等待用户在真实页面完成操作,完成后将浏览器的 cookie 写入 cookie 库,并返回登录后页面内容。无可用浏览器时 MUST 优雅降级(返回错误/提示)而非崩溃。

#### Scenario: 起浏览器等用户完成
- **WHEN** JS 执行 `net.startBrowserAwait('https://site/login')`,用户在弹出的浏览器里登录后关闭/确认
- **THEN** 浏览器产生的 cookie 被写入 cookie 库,调用返回登录后页面

### Requirement: 结构化多步编排(命名捕获与引用)

多步传值 SHALL 使用**结构化字段**而非字符串 DSL,以保持 ai-friendly、可被 booksource-generator 生成。请求级 `vars`(`{ name: <Rule> }`)在该请求响应后对每个 `Rule` 求值并存入作用域变量;后续请求的 URL/header/body 用现有 `{{name}}` 模板引用。捕获 MUST 先于引用它的步骤求值;变量按章节→书籍→书源作用域级联。引擎 MUST NOT 要求书源作者书写 Legado 式 `@put:/@get:` 嵌套字符串 DSL。

#### Scenario: 列表页捕获 token 带入详情页(结构化)
- **WHEN** 列表请求声明 `vars: { token: <从响应抽取 token 的 Rule> }`,详情请求 URL 模板含 `{{token}}`
- **THEN** 引擎先对列表响应求值 token 存入变量,详情请求 URL 携带正确的 token 值

#### Scenario: 不使用字符串 DSL
- **WHEN** 书源作者表达跨请求传值
- **THEN** 用结构化 `vars` + `{{}}` 模板即可,无需写 `@put:{...}`/`@get:{...}`

### Requirement: per-source 持久状态文件

引擎 SHALL 为每个书源维护一份持久状态文件 `~/.novel/source-state/{url_md5}.json`,统一存放 `kv`、`variable`、`login_header`、`login_info`(加密)、`cookies`。该文件 MUST 在作用域退出时自动保存(Drop-autosave),并支持按书源清理。

#### Scenario: 状态跨会话保留
- **WHEN** 登录写入 loginHeader 后退出 app,再次启动并加载同一书源
- **THEN** loginHeader 仍存在,无需重新登录

### Requirement: host 能力的 feature 门控与授权

host 对象的网络与状态能力 SHALL 由编译期 feature 门控;未启用时引擎退化为现有纯沙箱(只注入 result/baseUrl/vars/crypto)。运行时,书源**首次使用网络能力** SHALL 需要用户授权(可记住),拒绝则降级。

#### Scenario: 未启用 feature 时退化为纯沙箱
- **WHEN** 未启用 host 桥 feature 构建,书源 JS 调用 `net.ajax`
- **THEN** 该方法不存在,行为与现有纯沙箱一致(向后兼容)

#### Scenario: 首次联网需授权
- **WHEN** 某书源 JS 首次发起 `net.ajax` 且用户未授权
- **THEN** 引擎请求用户授权;用户拒绝则该能力降级、不发起请求
