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

### Requirement: 结构化多步编排(前置请求链 + 命名捕获与引用)

多步传值 SHALL 使用**结构化字段**而非字符串 DSL,以保持 ai-friendly、可被 booksource-generator 生成。每个 op(search/explore/bookInfo/toc/content)MAY 声明一条**有序前置请求链** `prelude`(`PreStep` 数组),在主请求/分页之前按数组顺序串行执行;每个 `PreStep` MAY 声明 `capture`(`{ name, value: <Rule>, scope }` 数组),在该步响应后对每个 `value` 规则求值,得 `String` 写入 `scope`(`chapter`|`book`|`source`,默认 `chapter`)作用域变量。`Request.vars`(`{ name: <Rule> }`)等价对 search 主请求响应的 `chapter` 级捕获。后续步骤与抽取规则一律用现有 `{{name}}` 模板引用。捕获 MUST 先于引用它的步骤求值(由「数组顺序 + 响应后捕获」两条物理保证,无需依赖图);变量按 章节→书籍→书源 作用域级联回退;空串捕获 MUST NOT 写入作用域层。引擎 MUST NOT 要求书源作者书写 Legado 式 `@put:/@get:` 嵌套字符串 DSL,也 MUST NOT 用 `#[serde(flatten)]` 内嵌 `Request`(会令 `deny_unknown_fields` 校验失效)。

#### Scenario: 前置请求链捕获 csrf 带入主请求(结构化)
- **WHEN** toc 声明 `prelude: [{ url: '/api/prepare', capture: [{ name: 'csrf', value: <Rule> }] }]`,主目录请求的 URL/规则含 `{{csrf}}`
- **THEN** 引擎先跑 prepare、捕获 csrf 写 chapter 层,主目录请求带正确 csrf;`prelude` 为空时引擎走与现状逐字节等价的旧路径

#### Scenario: 列表 token 带入详情(跨 op)
- **WHEN** `search.prelude` 捕获 token 标 `scope: 'source'`,同一会话用户选书后引擎调 `book_info`
- **THEN** 因 source 层经 Engine `Arc` 共享,`book_info` 主请求自动取到 `{{token}}`;跨会话/历史续读(全新 Engine、source 层空)时,`book_info`/`toc`/`content` 各自声明 `prelude` + `skipIfPresent` 幂等重取(search 阶段无 per-book 载体,token MUST NOT 落 `book` 层)

#### Scenario: token 复用避免每章重抓
- **WHEN** `content.prelude` 的某步声明 `skipIfPresent: ['token']` 且 token 已在作用域内非空
- **THEN** 引擎跳过该步、不重复发请求(`source`/`book` 层 token 复用,缓解 N×RTT)

#### Scenario: 不使用字符串 DSL
- **WHEN** 书源作者表达跨请求传值
- **THEN** 用结构化 `prelude`/`capture` + `{{}}` 模板即可,无需写 `@put:{...}`/`@get:{...}`

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
