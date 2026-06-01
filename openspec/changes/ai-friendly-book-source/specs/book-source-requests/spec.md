## ADDED Requirements

### Requirement: 结构化 HTTP 配置

书源 SHALL 提供结构化 `http` 块:`headers`(键值)、`cookies`(键值)、`charset`(auto|utf-8|gbk|gb18030|big5)、`timeout`、`retry`(max/backoffMs)、`rateLimit`(maxCount/perMs)。所有取页操作 MUST 应用该配置。

#### Scenario: 自定义 UA 头随请求发送

- **WHEN** `http.headers` 含 `User-Agent`
- **THEN** 引擎发出的每个请求都带该头

#### Scenario: GBK 站点正确解码

- **WHEN** `http.charset` 为 `gbk`,目标页为 GBK 编码
- **THEN** 抓取的正文/标题被正确解码为可读文本(不出现乱码)

### Requirement: Cookie 注入与会话复用

系统 SHALL 支持三种 Cookie 来源,统一作用于后续请求:① `http.cookies` 静态写入;② 可选 `http.warmup` 先 GET 指定页、借客户端 cookie store 预热会话 cookie;③ 运行时由外部注入(为反爬后端预留,落点同为 `http.cookies` / cookie store)。同一会话内的后续请求 MUST 复用已获得的 cookie。

#### Scenario: 预热后复用会话 cookie

- **WHEN** `http.warmup` 含首页 URL,随后请求搜索/详情页
- **THEN** 引擎先访问首页获取 cookie,后续请求自动携带这些 cookie

#### Scenario: 静态 cookie 生效

- **WHEN** `http.cookies` 含某键值
- **THEN** 该 cookie 出现在所有请求中

### Requirement: 单请求方法/参数覆盖

搜索/浏览等操作 SHALL 用 `Request` 表达:`url`(规则或模板)、`method`(GET|POST)、可选 `body`、可选 `headers` 覆盖、可选 `vars`(命名捕获供模板使用)。系统 MUST 支持 POST 搜索。

#### Scenario: POST 搜索

- **WHEN** `search.request` 为 `{ "url": "...", "method": "POST", "body": "key={{key}}" }`
- **THEN** 引擎以 POST + 该 body 发起搜索请求

#### Scenario: 请求级头覆盖全局头

- **WHEN** `search.request.headers` 设置了与 `http.headers` 同名的头
- **THEN** 该请求使用请求级的值
