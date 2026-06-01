## Context

当前书源(`crates/parse-book-source`)是 Legado 风格:每条规则是一个紧凑字符串 DSL,由 `analyzer_manager.rs` 解析(`split_rule_resolve` 按 `@css:`/`@json:`/`##`/`&&`/`||`/`{{}}`/`@put`/`@get` 拆分,右往左扫描)。取页是 `HttpClient`(reqwest + rustls + cookie_store),`HttpConfig{timeout, header, rateLimit}`。循环只有 `RuleContent::More{content,nextContentUrl,start,end}`,用计数终止(`end` 解析失败会死循环)。

决策前提(用户已拍板):**不兼容旧书源**。因此可以丢弃整套紧凑串解析层,设计一套干净、显式、AI 原生的 v2 格式。

## Goals / Non-Goals

**Goals:**
- 规则可被 JSON-Schema 完整描述,从而被「约束解码 / tool use」在生成期 100% 保证形状合法。
- 每个抽取意图拆成独立、显式、可单独校验的字段(降 AI 出错率)。
- 一等公民地表达:**Cookie / 自定义请求头 / POST body / GBK 编码 / 重试 / 限速**。
- 统一、**可终止**的循环/分页原语(修掉死循环隐患)。
- 内建**黄金样例 + 可执行不变量**,支撑"生成→验证→自愈"闭环(研究里的"路线 B")。
- 引擎遍历结构化对象即可执行,**不再需要任何字符串 DSL 解析**。

**Non-Goals:**
- 不保留/导入 Legado 紧凑字符串格式(如需,另写离线 `legado→v2` 工具,不进运行时)。
- 不在本 change 内做反爬取页(wreq/TLS 指纹、FlareSolverr)——正交,只预留 `http.cookies` 注入位与 `via:"xpath"` 占位。
- 不做"每页调 LLM"的运行时抽取(路线 A);AI 只在造/修书源时介入。

## Decisions

### D1:统一的递归 `Rule` 类型(格式的心脏)
一条规则对「当前上下文」(一段 HTML / 一个 JSON 值 / 一段已抽取文本)求值,产出字符串(值规则)或一组元素(列表规则,如 `bookList`/`chapterList` —— 由字段角色决定"取一个还是取多个",规则形状统一)。

**叶子规则**(显式字段,取代紧凑串):
```jsonc
{
  "via": "css",          // enum: css | xpath | json | regex | raw   (默认 css)
  "select": ".title",    // 选择器/JSONPath/正则;语义随 via
  "index": 0,            // 可选:取第 N 个匹配,负数从末尾
  "extract": "text",     // enum: text|ownText|html|innerHtml|outerHtml | {"attr":"href"}  (默认 text)
  "clean": [             // 可选:有序后处理流水线(取代 ##正则##替换)
    { "regex": "\\s+", "replace": " " },
    { "trim": true },
    { "prepend": "https://host" }
  ]
}
```
- `via:"raw"`:不选择,直接用当前上下文值(只跑 clean)。
- `via:"regex"`:`select` 为正则,取首个捕获组/匹配。
- `extract:{"attr":"href"}`:取属性(href/src/data-src/content…)。

**组合子**(取代 `||` / `&&` / `{{}}`;特意**不用** `anyOf/allOf/const` 这些 JSON-Schema 关键字名以免撞名,按"出现哪个键"区分,JSON-Schema 用 `oneOf` + `required` 判别,互斥不歧义):
```jsonc
{ "firstOf": [ <rule>, <rule> ] }               // 取首个非空 —— 即回退/规则级冗余/自愈
{ "concat":  [ <rule>, <rule> ], "join": " " }  // 拼接非空
{ "literal": "https://www.bilixs.com" }         // 字面量
{ "template": "{{base}}/search?q={{key}}&pg={{page}}" }  // 变量插值
```
> `firstOf` 同时承担"回退"与"自愈":同一字段配多个候选选择器,首个命中即用;线上某个失效自动退下一个。无需单独的 `fallbacks` 键。

**变量**:`template` 可插 `{{key}}`(搜索词)、`{{page}}`/`{{pageSize}}`(分页)、以及请求级 `vars` 里命名捕获的值(取代 `@put`/`@get`)。

### D2:顶层结构(显式分区)
```jsonc
{
  "schema": "trnovel-booksource/v2",
  "name": "哔哩小说", "group": "测试",
  "url": "https://www.bilixs.com",          // base,用于相对链接解析
  "http": { /* D3 */ },
  "search":   { "request": <Request>, "list": <Rule>, "item": <BookRules> },
  "explore":  { "categories": [ {"title": "...", "url": "<template>"} ], "list": <Rule>, "item": <BookRules> },
  "bookInfo": <BookRules>,                   // 从书详情页抽
  "toc":      { /* D4 list/name/url/isVolume/nextPage/maxPages */ },
  "content":  { /* D4 value/nextPage/maxPages */ },
  "samples":  [ /* D5 */ ]
}
```
`BookRules = { name, author, cover, intro, kind, lastChapter, tocUrl, wordCount }`,每项是 `Rule`(可省略=空)。

### D3:结构化请求与 Cookie
```jsonc
"http": {
  "headers": { "User-Agent": "Mozilla/5.0 …", "Referer": "https://www.bilixs.com/" },
  "cookies": { "sessionid": "…" },     // 静态 cookie;也是运行时注入 Cloudflare clearance cookie 的落点
  "warmup":  [ "https://www.bilixs.com/" ],  // 可选:先 GET 这些页,借 cookie_store 预热会话 cookie
  "charset": "auto",                    // auto|utf-8|gbk|gb18030|big5(部分站 GBK)
  "timeout": 15000,
  "retry":   { "max": 2, "backoffMs": 500 },
  "rateLimit": { "maxCount": 3, "perMs": 1000 }
}
```
单请求可覆盖:`Request = { "url": <Rule|template>, "method": "GET"|"POST", "body": <Rule>, "headers": {…}, "vars": { "<name>": <Rule> } }`。
**Cookie 三种来源统一落到 `http.cookies` + 客户端 cookie_store**:① 静态写死;② `warmup`/前序请求自动累积(cookie_store 已开);③ 运行时由外接反爬后端注入(未来 change)。这回应"某些可能需要 cookie"。

### D4:有界循环/分页(回应"有些需要循环")
```jsonc
"content": { "value": <Rule>, "nextPage": <Rule>, "maxPages": 50 }
"toc":     { "list": <Rule>, "name": <Rule>, "url": <Rule>, "isVolume": <Rule>,
             "nextPage": <Rule>, "maxPages": 200 }
```
**终止契约**:循环抓取 `nextPage` 求值得到的 URL,拼接结果,直到 `nextPage` **为空** 或 达到 `maxPages`(默认硬上限,如 100)。**绝不依赖解析出的 `end` 计数** → 根除现有 `RuleContent::More` 的死循环隐患。`nextPage` 省略即单页(等价旧 `One`)。
- 多页正文:`content.nextPage`。
- 多页目录/"加载更多":`toc.nextPage`。
- 搜索/浏览分页:由 `{{page}}` 模板 + 阅读器请求第 N 页驱动(非循环)。

### D5:黄金样例 + 验证回路(让 AI 造书源真能用)
```jsonc
"samples": [
  { "bookUrl": "/novel/guzhenren.html",
    "expect": { "name": "蛊真人", "minChapters": 2000, "volumes": 8, "minContentChars": 500 } }
]
```
引擎提供 `verify(source)`:跑 samples,对每个阶段断言**可执行不变量**——搜索/浏览:≥1 项且 `name` 非空、`bookUrl` 像 URL;bookInfo:`name` 非空;toc:章节数达标、章节 URL 合法;content:字数 ≥ 阈值且不是挑战页。失败时返回「断言名 + 期望 vs 实际 + 命中的 HTML 片段」结构化反馈。**同一套 verify 既用于生成期校验(AI loop until green),也用于运行期 self-heal(失效→退 anyOf 候选→再不行标记书源失效/触发重生成)。**

### D6:引擎=遍历执行,删除 DSL 解析层
求值器对 `Rule` 做递归下降:叶子→按 `via` 调底层原语(CSS=scraper、json=jsonpath-rust、regex=regex、xpath=可选 skyscraper);组合子→anyOf 短路、allOf 收集 join、template 插值。**`split_rule_resolve`/`SPLIT_RULE`/`@put`/`@get`/`{{}}` 字符串处理全部删除**;底层 `html.rs`/`json.rs` 的取值原语保留并瘦身(只暴露 select/attr/text/html/regex)。

### D7:整库重写的模块组织(分层 / 端口-适配器)
依赖只向内指(domain ← use case ← mechanism),配置(source)作为输入数据:
```
crates/parse-book-source/src/
  lib.rs            // 公开 API:BookSource(配置)、Engine(用例入口)
  model/            // 纯领域类型:Book / BookInfo / Chapter / Volume(无 IO、无 serde 规则逻辑)
  source/           // v2 配置(serde):
    rule.rs         //   Rule AST(Leaf / FirstOf / Concat / Literal / Template)
    http.rs         //   Http / Request / Cookie / Charset / Retry / RateLimit
    sample.rs       //   samples + 期望不变量
  eval/             // 规则解释器(纯函数,可离线测试)
    value.rs        //   Value: Text | List<Node> | Json
    context.rs      //   Context: HtmlNode | JsonValue | Text(当前求值上下文)
    evaluator.rs    //   递归下降求值 Rule
  backend/          // 抽取后端(Strategy):
    mod.rs          //   trait ExtractBackend
    html.rs         //   scraper/dom_query;  json.rs(jsonpath); regex.rs;  xpath.rs(feature)
  fetch/            // trait Fetcher + ReqwestFetcher(默认);留口给 wreq / FlareSolverr 适配器
  engine/           // 用例:search / explore / book_info / toc / content + paginator.rs
  verify/           // samples 运行器 + 不变量断言 + 失败反馈
  error.rs          // 分层 thiserror 错误
```

### D8:落到的设计模式(为什么用、解决什么)
- **解释器 + 组合(Interpreter + Composite)**:`Rule` 是 AST(叶子 + `FirstOf`/`Concat`/`Literal`/`Template` 组合子),`Evaluator` 递归求值。取代字符串 DSL 解析——结构即语法树,无解析步骤、无解析错误。
- **策略(Strategy)——抽取后端**:`trait ExtractBackend { select / extract }`,`via` 选择 css/json/regex/xpath 实现。新增 XPath = 加一个策略,引擎零改动(开闭原则)。
  - **HTML `select` 语义 = self-or-descendant**:在「当前元素」上下文里,`select` 既匹配后代、也匹配元素自身(把元素当 fragment、根参与匹配)。这正是旧引擎能用 `select:"a" + attr:href` 取「列表项自身的 href」、用 `select:"h2"` 判「该项是不是卷标题」的机制,v2 沿用。`select` 省略 = 元素自身。
- **端口-适配器 / 依赖倒置(Ports & Adapters)——取页**:`trait Fetcher`,默认 `ReqwestFetcher`;反爬(wreq TLS 指纹、FlareSolverr 注入 cookie)只是另一个 `Fetcher` 适配器,**引擎与抓取实现解耦**。这同时带来最大的质量红利:**引擎可用 fixture HTML 离线单测**(注入 mock Fetcher / 直接喂保存的页面),规则正确性不依赖真实网络。
- **模板方法(Template Method)——用例骨架**:search/explore/toc/content 共享「取页 → 选列表/值 → 映射 item 规则 → 可选分页循环」骨架;差异点(列表规则、item 规则)作为参数。`Paginator` 把 `nextPage`+`maxPages` 循环抽成可复用、可终止的组件。
- **建造者(Builder)**:`EngineBuilder` 组装 `BookSource` + `Fetcher` + 后端集合,便于注入测试替身与可选 feature。
- **新类型 + 类型化错误**:用领域类型与 `thiserror` 分层错误替代到处 `anyhow::anyhow!("…")`,让失败可被 `verify`/self-heal 结构化消费。

### D9:代码质量基线
- **可离线测试**:借 `Fetcher`/`ExtractBackend` trait,核心规则求值与各操作用 fixture(保存的 bilixs 页面)单测,不打网络;`verify` + `samples` 作集成回归。
- 每个 `via`/组合子/clean 变换都有针对性单测;`Paginator` 的终止(空 nextPage / 命中 maxPages)单测。
- 严格 clippy(`-D warnings`)、rustdoc、模块边界清晰(domain 不依赖 reqwest/scraper)。
- 公开 API 表面最小化:对外只露 `BookSource`、`Engine`/`EngineBuilder`、`model::*` 与错误类型。

### D10:异步与错误处理约定(Rust 最佳实践落地)
本节是对 rust-best-practices(ch1/4/6/9)与 rust-async-patterns 的**具体裁决**,作为 apply 阶段的硬约束:

- **分派选择(ch6 "能静则静,该动才动")**:
  - `Fetcher` = **动态分派** `Arc<dyn Fetcher>` + `#[async_trait]`(或 `trait_variant`)。理由:运行时可热插拔后端(reqwest / wreq / FlareSolverr),正是 ch6 点名该用 dyn 的"插件式/稳定接口抽象"场景;取页非热路径,vtable 成本相对网络延迟可忽略。`dyn` 仅在 API 边界出现(`Arc` 包裹)。
  - `ExtractBackend` = **静态分派**:`via` 是闭集枚举,用 `match via { Css=>…, Json=>… }` 调具体实现;新增 XPath = 加一个 feature 门控的 match 臂,引擎零改动。
- **async/sync 隔离**:`eval`(规则求值)**保持同步**——抽取是 CPU 活、无 IO;只有 `engine` 的取页是 `async`。绝不把 `eval` 染成 async。
- **不跨 await 持锁 / 不要 `Arc<Mutex<Parser>>`(async skill 红线)**:`Engine` 设计为**廉价 `Clone`**(内部 `Arc<dyn Fetcher>` + `Arc<BookSource>`),操作取 `&self`,**移除**现有 `NetworkNovel` 里 `Arc<Mutex<BookSourceParser>>` 跨 `.await` 持锁的反模式。需要可变小状态(如 `temp`/变量表)时,用方法内局部值或 `&mut self`,不在锁内 await。
- **大页面解析用 `spawn_blocking`**:目录页可达 ~800KB,scraper 解析是 CPU 阻塞;在异步上下文里用 `tokio::task::spawn_blocking` 跑解析,避免卡执行器。小页面可直接解析。
- **限速/重试/超时全走 tokio 时间**:`tokio::time::{sleep, timeout}`,**禁用 `std::thread::sleep`**;`rateLimit` 用 tokio `Semaphore`/令牌桶,`retry` 用带 `backoffMs` 的异步退避;每个请求套 `timeout`。
- **错误(ch4)**:**库内不用 anyhow**;分层 `thiserror`——`FetchError`/`ExtractError`/`EvalError`/`VerifyError` 经 `#[from]` 汇入顶层 `BookSourceError`。错误 `Send + Sync + 'static`(跨 await/任务)。`verify` 的失败反馈是结构化类型,非字符串。生产路径**无 `unwrap`/`expect`**,用 `?` 与 `let-else`。
- **借用(ch1/ch3)**:`eval` 在借用的上下文(`&Html`/`&Value`/`&str`)上工作,避免对大 HTML 反复 `clone`;函数签名用 `&str`/`&[T]`;仅在确需所有权时 clone。
- **Send/Sync(ch9)**:async 路径只用 `Arc`(不用 `Rc`/`RefCell`);跨 `.await` 的值满足 `Send`。
- **依赖基线(重写采用最新 + 更优替代)**:HTML 抽取后端用 **`dom_query`(最新)替代 `scraper`**(jQuery 式 API + `:has()/:contains()/:has-text()` 扩展伪类,消化手写分支);JSONPath 用 **`jsonpath-rust` 1.x**(对齐 RFC 9535,从 0.7 破坏性升级);HTTP **保持 `reqwest` 0.12.x + rustls(ring)**——0.13 的 `rustls` 默认改用 aws-lc-rs(带 C 构建依赖),与本项目纯净静态二进制取向相悖,故不升 0.13;正则 **`fancy-regex` 0.18**;XPath 后端(feature)用纯 Rust `skyscraper`。async trait 用 `async-trait` 或 `trait_variant`(取最新)。

## Risks / Trade-offs

- **放弃 Legado 生态**:不能直接吃海量现成书源 → 缓解:格式更干净 + 由 AI 量产 v2 书源;必要时离线 `legado→v2` 转换器(一次性,不进运行时)。
- **规则层基本重写**:`parse-book-source` 改动大 → 缓解:底层取值原语(scraper/jsonpath)复用;先在 v2 bilixs 样例 + 校验回路上锁定行为再清理旧码。
- **结构化更冗长**:JSON 体积比紧凑串大 → 对 AI 无所谓(token 便宜),对人可由 UI/编辑器辅助;这是刻意的取舍(显式 > 紧凑)。
- **`oneOf` 判别歧义**:叶子与组合子若键重叠会让 schema/约束解码犯难 → 缓解:组合子用各自唯一键(anyOf/allOf/const/template)判别,叶子以 `select`/`via` 判别,互斥。
- **循环安全**:`maxPages` 必须有默认硬上限,且单页抓取设超时/重试上限,避免恶意/坏书源拖死;`verify` 的不变量也要能识别"挑战页/空页"避免把错误内容当正文。
- **XPath/反爬留白**:`via:"xpath"` 与 `http.cookies` 只是占位,真正实现(skyscraper / wreq / FlareSolverr)是后续独立 change;本 change 不得引入对应强依赖。
