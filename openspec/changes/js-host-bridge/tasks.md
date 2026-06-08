## 1. 决策与脚手架(P0,动手前定)

- [x] 1.1 host 桥门控 → **新增 `js-host` feature**(与 `js` 分开);待加进 `crates/parse-book-source/Cargo.toml`
- [x] 1.2 boa 网络桥 runtime → **`spawn_blocking` + 独立 current-thread runtime**(绝不在主 worker 内 block);实现时附最小不死锁验证
- [x] 1.3 凭据加密密钥 → **`machine-uid` 绑机器**派生 AES key(记录在 design)

## 2. per-source 持久状态(P1)

- [x] 2.1 `SourceState` 类型(`kv`/`variable`/`login_header`/`login_info` 加密/`cookies`);serde (反)序列化 — `state.rs`
- [x] 2.2 `load(path)`/`save(path)`(路径由调用方/app 给定,库**不硬编码** `~/.novel`,保持纯净);Drop-autosave 留待 Engine 接入时挂
- [x] 2.3 `login_info` 用 AES-256-CBC(复用 `transform::cipher`)+ `machine-uid` 派生密钥加解密;与明文 `login_header` 分桶
- [x] 2.4 TTL:`SourceState.expire_at` + `is_login_expired()` / `clear_login()`(清登录态保留 kv) / `purge_if_expired()`(读时校验);按书源清理对齐 `trn -c` 仍由 app 删文件
- [x] 2.5 单测:json round-trip、加密往返、缺失返回默认、save/load(`--features js-host` 58 passed)

## 3. host 对象骨架(P1)

- [x] 3.1 定义 `SourceHost` struct(`host.rs`):持有 `SourceState` / 专属 `Arc<dyn Fetcher>` / **独立 current-thread runtime** / `dirty` 标记;线程亲和(同线程组装求值析构)
- [x] 3.2 boa 注入:把同一 `SourceHost`(thread-local + RAII guard)以 `source` 与 `net` 两名**对象**注入全局;用 `NativeFunction::from_fn_ptr` 注册各方法回调进 Rust(绕开 boa GC `Trace` 约束)
- [x] 3.3 feature 门控:未启用 `js-host` 时退化为现有纯沙箱(只注入 result/baseUrl/vars/crypto),向后兼容(默认构建已验证不含 host)
- [x] 3.4 host API 白名单评审:仅网络/状态/cookie(浏览器待 7.x),**确认无文件系统/进程能力**(单测断言 require/process/exec/readFile 均 undefined)
- [x] 3.5 单测:`source` 与 `net` 同一 host 实例(`source.put`/`source.get` 与 `net.getCookie` 读写同一 `SourceState`)

## 4. JS 网络桥(P1)

- [x] 4.1 `net.ajax(url)`:复用取页管线(host 独立 runtime `block_on` `Fetcher::fetch`),返回响应体字符串;在 spawn_blocking 线程上不死锁(本地回显服务端到端验证)
- [x] 4.2 `net.connect(url, extraHeadersJson?)`:经 `Fetcher::fetch_full`(新增,默认仅 body / `ReqwestFetcher` + `EscalatingFetcher` 均覆盖回 status+headers)返回**结构化属性**对象 `{body, code, headers}`(优于 Legado 的 `body()/headers()` 方法形式,契合 ai-friendly);额外头非法 JSON 抛错(不静默吞);timeout 待补
- [ ] 4.3 ajax/connect 的 url 支持再带 `,{option}` 并内嵌 `{{}}`/`<js>`(递归走 URL 解析管线)
- [x] 4.4 JS 发起的请求自动附加当前书源 loginHeader(JWT/Cookie 同路径);引擎自身请求的 loginHeader 合并见 6.3,cookie 库按域名归并合并见 10.x
- [x] 4.5 失败以可被 JS 捕获的方式返回(`EvalError::Host` → JS 异常,`try/catch` 可捕获,不使整段求值崩溃)
- [ ] 4.6 `net.ajaxAll(urls)` 并发(复用现有 RateLimiter)— 可选
- [x] 4.7 `net.post(url, body, extraHeadersJson?)`:POST 表单/JSON 登录(spec source-auth 要求),返回 `{body, code, headers}`;出站头剥除 CR/LF(防注入 + 防 `\n`-连接 Cookie 回写致 reqwest 构建失败)

## 5. 状态 / cookie 桥(P1)

- [x] 5.1 `source.put/get`(写返回值、读缺失返回空串)+ `getVariable/putVariable`(单槽);状态写入置 `dirty` 供调用方落盘(命名空间由 per-source 状态文件提供)
- [x] 5.2 `net.getCookie(domain, key?)` 读 cookie 库(给定 key 取该 cookie 值,否则返回整串)
- [x] 5.3 单测:put/get 跨调用 + 缺失返回空串、getVariable/putVariable 单槽、getCookie 按域名/键

## 6. 登录态注入(P1,核心)

- [x] 6.1 `source.putLoginHeader`(JSON 整体设置)/`getLoginHeader`(JSON 串)/`getLoginHeaderMap`(JS 对象)/`removeLoginHeader`(清空);写入置 dirty
- [x] 6.2 `putLoginHeader` 检测 header map 含 `Cookie`/`cookie` 字段时,按二级域名归并写入 cookie 库(`net.getCookie` 可读回)
- [x] 6.3 引擎构造每个请求时并入 `login_header`(对齐 spec「引擎构造每个网络请求时」;`Engine::with_login_header` 注入,`get_req`/search 统一合并;loginHeader 覆盖书源同名头)。注:落在 `engine.rs`(它构造请求并知 source 登录态)而非 `fetch.rs`,合并顺序「书源 header → loginHeader」
- [x] 6.4 单测:JWT(`Authorization: Bearer`)与 Cookie 两种 loginHeader 都被引擎每请求携带;空 loginHeader 不注入(向后兼容);Cookie 同步到库见 6.2

## 7. headful 浏览器登录(P1)

- [ ] 7.1 `browser.rs` 新增 `login(url, options)`:复用 `solve()` 的 headful 启动 + 持久 profile 骨架
- [ ] 7.2 登录态提取:`get_cookies`(含 HttpOnly)+ `page.evaluate("localStorage")` 取 JWT
- [ ] 7.3 成功判定:目标 cookie/localStorage 键出现 或 用户在 TUI 点「登录完成」;同步原语用 tokio `Notify`/`oneshot`(替代 LockSupport park/unpark)
- [ ] 7.4 `net.startBrowserAwait(url, title?)` 桥;成功后把浏览器 cookie 写入 cookie 库;返回登录后页面 HTML(可选 `refetchAfterSuccess`:用 cookie 走 reqwest 重抓)
- [ ] 7.5 无浏览器/未授权降级(对齐 browser-fetcher 既有策略)

## 8. 脚本登录编排(P1)

- [x] 8.1 `source.rs` 新增 `loginUrl` 字段(camelCase)+ `has_login()` + `get_login_js()`(剥 `@js:`/`<js>` 前缀);schema 已重新生成(schema_sync 通过)
- [x] 8.2 `host::run_login()`:拼固定壳 `if(typeof login=='function'){login.apply(this);}` 调用书源 `login()`,host 注入,登录态写回 state(端到端测:login() 经 net.ajax 取 token → putLoginHeader)
- [x] 8.3 `source.putLoginInfo`(AES 加密落盘,机器绑定密钥)/`getLoginInfo`(解密明文)/`getLoginInfoMap`(解密后解析为 JS 对象);写入置 dirty
- [ ] 8.4 端到端:用番茄会员场景手动验证「脚本登录 → putLoginHeader → 读全本正文」

## 9. TUI 登录入口(P1)

- [ ] 9.1 `hasLogin` 判定(loginUrl/loginUi 非空);在 root crate 加「书源登录」入口/页面
- [ ] 9.2 触发 headful 登录 / 脚本登录;登录态按 url_md5 持久、跨会话复用
- [ ] 9.3 简化版过期处理:请求判失效时提示用户重新登录

## 10. cookie 持久化升级(P2)

- [x] 10.1 cookie 按**注册域(eTLD+1)**归并:`psl`(纯 Rust,编译期内嵌,**转非可选依赖**)+ `cookie.rs::registrable_domain()` 正确处理 `example.com`/`example.co.uk`/`site.com.cn`(IP/单标签回退);全 crate 统一用之
- [x] 10.2 `CookieJar`(`cookie.rs`)按注册域归并 + **session/persistent 分离**:无 `Expires`/`Max-Age` 为 session(仅内存,`persistent()` 不导出),`Max-Age<=0` 删除
- [x] 10.3 `enabledCookieJar`:引擎 `run_request` 响应后回灌 `Set-Cookie`(多条 `\n` 分);`with_cookies`/`persistent_cookies` 供 app 跨会话载入/落盘
- [x] 10.4 出站请求按 URL 注册域并入库 cookie + loginHeader 的 `Cookie` 去重合并(host `net.*` 与引擎 `apply_auth` 双侧);临时 urlOption cookie 优先待 4.3
- [ ] 10.5 `cf_clearance` 并入统一库 —— **刻意暂缓**:cf_clearance 必须与签发 UA 配对(design D6),放进不带 UA 的通用库再发会被 CF 拒;现留在 `EscalatingFetcher` 内(会话内有效),跨会话持久需 UA 协同设计
- [x] 10.6 单测:注册域 publicsuffix 边界、session/persistent 分离、`Max-Age=0` 删除、from_persistent 往返、引擎 enabledCookieJar 回灌→再发→persistent 导出 + 关闭不回灌

## 11. 结构化多步编排(P2,无字符串 DSL)

- [ ] 11.1 结构化命名捕获:请求级 `vars`(`{name: <Rule>}`)在响应后对每个 Rule 求值存入变量;**不引入 Legado `@put:/@get:` 字符串 DSL**(保持 ai-friendly 结构化 schema),引用沿用现有 `{{name}}` 模板
- [ ] 11.2 接通已定义未接线的 `Request.vars`:前置请求捕获值带入后续 URL/header/body 模板;明确「捕获先于引用」时序
- [ ] 11.3 变量作用域章节→书籍→书源级联回退;书籍级随 per-book 快照持久化
- [ ] 11.4 `book-source.schema.json` 为 `vars` 标注 schema(供 booksource-generator/AI 识别生成)
- [ ] 11.5 单测:列表页捕获 token 带入详情页;作用域就近落点

## 12. 进阶能力(P3)

- [ ] 12.1 `loginUi` RowUi 解析 + TUI 渲染(text/password 掩码/select/toggle);收集值加密存 loginInfo
- [x] 12.2 `loginCheckJs`:引擎每个网络方法响应后执行(`result`=响应),空/`false`/`0` 判失效 → `BookSourceError::LoginExpired`(`is_login_expired()` 供 app 提示重登);D10 第一版不自动重发
- [x] 12.3 `concurrentRate`(`"N/ms"` 或纯间隔)解析为 RateLimiter,`http.rateLimit` 缺省时启用
- [ ] 12.4 jsLib 书源级共享 JS 作用域(boa 用同一 Realm 或源码前置拼接模拟)

## 13. schema / 文档 / 质量(贯穿)

- [x] 13.1 `book-source.schema.json` 新增字段(`loginUrl`/`loginUi`(RowUi/RowUiType)/`loginCheckJs`/`enabledCookieJar`/`concurrentRate`)并重新生成 + schema_sync 通过
- [x] 13.2 `booksource-generator` skill 补「登录与多步编排(host 桥)」章节(source/net/crypto 对象、新字段表、脚本登录范式示例)+ 同步 references 的 schema
- [ ] 13.3 安全审查:host API 白名单(无 fs/exec)、首次联网授权流程、凭据加密、登录态文件权限
  - 决策(对抗式审查结论):`net.*` 对**任意 URL** 附带 loginHeader 属 spec 既定设计(JWT/Cookie 同路径),**不**加同站校验(会破坏 www→api 子域登录);真正的凭据外泄防线是本任务的「首次联网需授权」门 + 文件权限收紧,在此统一落实
  - 已做加固(本轮审查后):`machine-uid` 空/平凡值拒绝(防公开可计算密钥伪加密);出站 header 剥除 CR/LF(防注入)
- [x] 13.4-sec 文件权限:`SourceState::save()` 在 unix 以 **0600** 落盘(`OpenOptions.mode` 避免新文件 0644 窗口 + 显式收紧已存在文件),保护 login_header 明文 + cookie
- [ ] 13.4 全套绿:`cargo test --all-features --workspace`、`clippy -D warnings`、`fmt --check`、`doc -D warnings`
