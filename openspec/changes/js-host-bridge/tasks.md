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

- [x] 7.1 `browser.rs` 新增 `login(url, options)`:复用 `solve()` 的 headful 启动 + 持久 profile 骨架
- [x] 7.2 登录态提取:`get_cookies`(含 HttpOnly)+ `page.evaluate("localStorage")` 取 JWT
- [x] 7.3 成功判定:目标 cookie/localStorage 键出现 或 用户在 TUI 点「登录完成」;同步原语用 tokio `Notify`/`oneshot`(替代 LockSupport park/unpark)
- [ ] 7.4 `net.startBrowserAwait(url, title?)` 桥;成功后把浏览器 cookie 写入 cookie 库;返回登录后页面 HTML(可选 `refetchAfterSuccess`:用 cookie 走 reqwest 重抓)
- [x] 7.5 无浏览器/未授权降级(对齐 browser-fetcher 既有策略)

## 8. 脚本登录编排(P1)

- [x] 8.1 `source.rs` 新增 `loginUrl` 字段(camelCase)+ `has_login()` + `get_login_js()`(剥 `@js:`/`<js>` 前缀);schema 已重新生成(schema_sync 通过)
- [x] 8.2 `host::run_login()`:拼固定壳 `if(typeof login=='function'){login.apply(this);}` 调用书源 `login()`,host 注入,登录态写回 state(端到端测:login() 经 net.ajax 取 token → putLoginHeader)
- [x] 8.3 `source.putLoginInfo`(AES 加密落盘,机器绑定密钥)/`getLoginInfo`(解密明文)/`getLoginInfoMap`(解密后解析为 JS 对象);写入置 dirty
- [ ] 8.4 端到端:用番茄会员场景手动验证「脚本登录 → putLoginHeader → 读全本正文」

## 9. TUI 登录入口(P1)

> app 启用 `js-host`(用户拍板:默认启用 + 13.3 首次联网授权门);登录态经 `cache::source_state`(`~/.novel/source-state/{url_md5}.json`,0600)持久化、由 `browser_assist::build_engine` 注入每个引擎(`with_login_header`/`with_cookies`)。登录动作在 `src/login.rs`(`script_login`/`browser_login`)。**整套 TUI 流程已编译+clippy+fmt+doc 全绿,但 TUI 交互需真终端运行时验收(你来跑)。**

- [x] 9.1 `hasLogin` 判定(`BookSource::has_login`=loginUrl/loginUi 非空,库侧单一口径,app 不再包装);书源管理页 `L` 键进 `/book-source-login`(`book_source_login.rs`;loginUi-only 而无脚本/loginUrl 的配置错误由登录页拦截提示)
- [x] 9.2 触发 headful 登录(`browser_login` 经 7.x `BrowserFetcher::login`)/ 脚本登录(`script_login` 经 host `run_login`,spawn_blocking);产物按 url_md5 持久化、`build_engine` 注入跨会话复用
- [x] 9.3 简化版过期处理:`loginCheckJs` 判失效抛 `LoginExpired`(Display=「登录态已失效,请重新登录」),book_detail 以 Display 展示提示重登(加载时 TTL 亦清登录态)

## 10. cookie 持久化升级(P2)

- [x] 10.1 cookie 按**注册域(eTLD+1)**归并:`psl`(纯 Rust,编译期内嵌,**转非可选依赖**)+ `cookie.rs::registrable_domain()` 正确处理 `example.com`/`example.co.uk`/`site.com.cn`(IP/单标签回退);全 crate 统一用之
- [x] 10.2 `CookieJar`(`cookie.rs`)按注册域归并 + **session/persistent 分离**:无 `Expires`/`Max-Age` 为 session(仅内存,`persistent()` 不导出),`Max-Age<=0` 删除
- [x] 10.3 `enabledCookieJar`:引擎 `run_request` 响应后回灌 `Set-Cookie`(多条 `\n` 分);`with_cookies`/`persistent_cookies` 供 app 跨会话载入/落盘
- [x] 10.4 出站请求按 URL 注册域并入库 cookie + loginHeader 的 `Cookie` 去重合并(host `net.*` 与引擎 `apply_auth` 双侧);临时 urlOption cookie 优先待 4.3
- [ ] 10.5 `cf_clearance` 并入统一库 —— **刻意暂缓**:cf_clearance 必须与签发 UA 配对(design D6),放进不带 UA 的通用库再发会被 CF 拒;现留在 `EscalatingFetcher` 内(会话内有效),跨会话持久需 UA 协同设计
- [x] 10.6 单测:注册域 publicsuffix 边界、session/persistent 分离、`Max-Age=0` 删除、from_persistent 往返、引擎 enabledCookieJar 回灌→再发→persistent 导出 + 关闭不回灌

## 11. 结构化多步编排(P2,无字符串 DSL)

> 设计见 design.md **D7-bis**(经独立方案评审 + 对抗式验证收敛)。骨架:每个 op 挂有序 `prelude: Vec<PreStep>`,每步对自身响应做结构化 `capture`,三级作用域 `chapter`(默认)/`book`/`source` 级联,引用一律 `{{name}}`。**不引入字符串 DSL、不 flatten 内嵌 Request。**

- [x] 11.1 `source.rs` 新增 `VarScope`(lowercase enum `chapter`|`book`|`source`,默认 `chapter`)、`Capture{name:String, value:Rule, #[serde(default)] scope}`、`PreStep{url,method,body,headers, capture:Vec<Capture>, skipIfPresent:Vec<String>}`(**显式字段,禁用 `#[serde(flatten)]`**:`Rule` untagged + `LeafRule` 非 deny_unknown_fields 会令 flatten 校验静默失效);全 `rename_all=camelCase` + `deny_unknown_fields` + schemars。给 `SearchOp/ExploreOp/TocRules/ContentRules` 各加 `prelude:Vec<PreStep>`(default+skip)。`Request.vars` 补 `skip_serializing_if`(类型不变,引擎首读=chapter 级捕获)。`BookSource.book_info`:`BookRules`→`BookInfoOp`(`BookRules` 全字段同名同序 + `prelude` + **`as_book_rules()` 视图**,供 `eval_book_info(&BookRules)` 复用,**修 Blocker1**)。
- [x] 11.2 `engine.rs` 命名捕获接线:新增私有 `ScopedVars{chapter:Vars, book:BTreeMap}` + `flatten(source)`(`source→book→chapter` overlay,**空串捕获不写层**);Engine 加 `book_vars:BTreeMap` + `source_vars:Arc<RwLock<BTreeMap>>`(三构造器初始化空,Clone 共享 source 层)+ 链式 `with_book_vars`/`with_source_vars` + 导出 `book_vars()`/`source_vars()`;`base_vars`→`base_scoped()`。新增 async `run_prelude(steps,&mut ScopedVars)`:`skipIfPresent` 全非空短路 → `resolve_url`+`apply_auth`+`run_request` → 按 `capture` 顺序非空写各层并并入 chapter(写锁 `await` 之后短临界、不跨 await)。
- [x] 11.3 五入口接线 + 作用域语义:search/explore 先 `run_prelude` 再发主请求并对 `Request.vars` 捕获(写 chapter);book_info 先跑 `book_info.prelude` 再 `fetch_checked`,经 `as_book_rules()` 复用 `eval_book_info`;toc/content 先 `run_prelude` 再 `fetch_pages`。`fetch_pages` 签名 `&Vars`→`&mut ScopedVars`,**toc 主体(name/url/is_volume)与 content 抽取一并改用 `flatten(scoped)`**(修 Non-blocking4)。`eval_list_items` 加 `vars` 参数,调用点传 `flatten` 结果(list/item 可见捕获)。**跨 op 修正**:search 阶段无 per-book 载体(`NetworkNovelCache` 选书后才 `TryFrom`)→ token **只落 `source`(同会话 Arc)/`chapter`,不落 `book`**(修 Blocker2);历史续读为全新 Engine、`source` 层空 → 需 token 的 toc/content 须各自 `prelude`+`skipIfPresent` 幂等重取(修 Blocker3)。
- [x] 11.4 schema 标注:重跑 `gen_schema` 生成 `book-source.schema.json`(**注意 `bookInfo` 的 `$ref` 由 `BookRules` 改指 `BookInfoOp` 属结构性改动、非纯追加**,须重生成过 schema_sync);为 `VarScope`/`Capture`/`PreStep`/`prelude`/`skipIfPresent` 写 description(三层寿命语义 + 默认 chapter 安全 + skipIfPresent 复用),供 booksource-generator/AI。
- [x] 11.5 root crate 接线 + 文档 + 测试:`NetworkNovelCache` 加 `#[serde(default)] book_vars:BTreeMap`,构造 Engine `.with_book_vars(...)`,调用后 `cache.book_vars=engine.book_vars()` 随 per-book 快照落盘(旧快照无字段靠 `#[serde(default)]` 兼容);booksource-generator 教 `prelude+capture` 固定形状、默认 chapter、token 才用 book/source、skipIfPresent 复用、**续读须自带 prelude 重取 source 级 token**;单测(MockFetcher):前置 csrf 链拼 `{{sign}}`、同会话 search→book_info 取 source 级 token、skipIfPresent 命中跳步、空串不写层、现有 `engine_toc_splits_volumes_offline`/`engine_merges_login_header`/`enabled_cookie_jar_*` 改 `&mut scoped` 后仍绿。本期**不做**逐页命名捕获(`pageCapture`)与 search 阶段的 `book` 级 token(降级为后续)。

## 12. 进阶能力(P3)

- [x] 12.1 `loginUi` RowUi 解析 + TUI 渲染(`book_source_login.rs`:text/password 掩码完整;select/toggle 暂以文本输入承载——proper 列表/开关控件为后续打磨);收集值 `login_info_json` → `set_login_info`(AES,机器绑定)存 loginInfo。**待真终端验收**
- [x] 12.2 `loginCheckJs`:引擎每个网络方法响应后执行(`result`=响应),空/`false`/`0` 判失效 → `BookSourceError::LoginExpired`(`is_login_expired()` 供 app 提示重登);D10 第一版不自动重发
- [x] 12.3 `concurrentRate`(`"N/ms"` 或纯间隔)解析为 RateLimiter,`http.rateLimit` 缺省时启用
- [ ] 12.4 jsLib 书源级共享 JS 作用域(boa 用同一 Realm 或源码前置拼接模拟)

## 13. schema / 文档 / 质量(贯穿)

- [x] 13.1 `book-source.schema.json` 新增字段(`loginUrl`/`loginUi`(RowUi/RowUiType)/`loginCheckJs`/`enabledCookieJar`/`concurrentRate`)并重新生成 + schema_sync 通过
- [x] 13.2 `booksource-generator` skill 补「登录与多步编排(host 桥)」章节(source/net/crypto 对象、新字段表、脚本登录范式示例)+ 同步 references 的 schema
- [x] 13.3 安全审查(结论见下):host API 白名单(无 fs/exec)、首次联网授权流程、凭据加密、登录态文件权限
  - **白名单(无 fs/exec)**✓:host 仅注入 `source`/`net`/`crypto`;单测(任务 3.4)断言 `require/process/exec/readFile` 均 undefined。
  - **首次联网授权**✓(结构性门控):app 里 host `net.*` 的**唯一可达路径是 `src/login.rs` 的 `run_login`**(用户按 `L` 显式登录触发);书源**规则 JS**(`Rule::Js`,search/toc/content)走纯沙箱 `crate::js::eval_js`(无 net)。即**正常浏览期间书源无法静默联网**——网络能力被「显式登录动作」门控,等价于「首次用网络能力需用户授权」。浏览器登录另有 browser-assist 的本次/总是/拒绝授权门。(如需「每书源首登额外确认弹窗」可后续小幅增补,非必需。)
  - **凭据加密**✓:`login_info` AES-256-CBC + machine-uid 派生密钥(空/平凡值拒绝,防公开可计算密钥);与明文 `login_header` 分桶。
  - **文件权限**✓:`SourceState::save()` unix 0600(见 13.4-sec)。
  - 已做加固:出站 header 剥除 CR/LF(host 与引擎双侧,写入侧亦净化);loginHeader 仅注入**同注册域**请求(host `net.*` 与引擎 `apply_auth` 双侧,共用 `cookie::merge_login_into_headers`)——防页面内容诱导的第三方绝对 URL 外泄 `Authorization`/Cookie;www→api 子域同注册域不受影响,登录域与 API 域分属不同注册域的书源会被静默跳过,如需支持留待 schema 级 `authDomains` 白名单(design/spec 中「每个请求自动 merge」的表述需按此收窄)。
- [x] 13.4-sec 文件权限:`SourceState::save()` 在 unix 以 **0600** 落盘(`OpenOptions.mode` 避免新文件 0644 窗口 + 显式收紧已存在文件),保护 login_header 明文 + cookie
- [ ] 13.4 全套绿:`cargo test --all-features --workspace`、`clippy -D warnings`、`fmt --check`、`doc -D warnings`
