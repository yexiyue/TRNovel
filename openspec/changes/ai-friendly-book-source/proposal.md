## Why

书源本质是「一份把网站 HTML 翻译成统一领域模型的声明式配置」,理应可由 AI 大批量生成/修复。当前(Legado 风格)格式有三个结构性障碍:

1. **规则是被过度压缩的字符串 DSL**(如 `@css:a@href##正则##替换`、`a||b`、`{{key}}`)——把 selector / 取值算子 / 正则 / 回退 / 变量 用分隔符位置编码挤在一行。这是 AI 一次成型正确率最大的敌人(研究:为 LLM 设计的「显式命名 + 规整语法」DSL 比紧凑写法在多步任务上正确率高约 40 个百分点),也无法被 JSON-Schema / 约束解码逐字段保护。
2. **缺少取页层表达力**:部分站点需要 **Cookie / 会话 / 自定义请求头 / POST body / GBK 编码**;当前 `header` 是扁平 JSON 字符串、`searchUrl` 是逗号分隔串,表达力弱且对 AI 不友好。
3. **循环/分页隐式且不安全**:`RuleContent::More` 用 `start/end` 计数循环,`end` 解析失败会**死循环**(已知隐患);多页目录、"加载更多"等场景没有统一、可终止的循环原语。

**决策:不向后兼容旧书源。** 放弃 Legado 字符串格式与其生态导入,换取一套干净、显式、AI 原生的 v2 格式。直接收益:不再需要脆弱的紧凑串解析器(`split_rule_resolve` / `SPLIT_RULE` 正则 / `@put`/`@get` 字符串拼接)——**结构化规则对象本身就是解析树**,引擎只需遍历执行,健壮性大幅提升。

## What Changes

- **结构化规则对象(唯一格式)**:规则从不透明字符串改为显式对象 `{ via, select, extract, clean, ... }`;组合子 `anyOf`(回退)/`allOf`(拼接)/`const`/`template`(变量插值)/`fallbacks`(规则级冗余)。每字段可独立配 JSON-Schema(`via`/`extract` 用 enum、`regex` 用 pattern 校验)、独立被约束解码保护、独立验证。
- **移除紧凑 DSL 与其解析层**:删掉 `analyzer_manager` 的 DSL 语法(`@css:`/`@json:`/`##`/`&&`/`||`/`{{}}`/`@put`/`@get` 字符串管线);保留底层取值原语(CSS 选择、属性/文本、正则替换、JSONPath)作为结构化规则的执行后端。**BREAKING:旧字符串书源不再被解析**(本项目尚无外部书源依赖,影响可控)。
- **结构化请求与 Cookie**:新增 `http` 块,显式表达 `headers` / `cookies`(静态 + 会话复用)/ `method` / `body` / `charset`(支持 GBK)/ `retry` / `rateLimit`。为后续接入 Cloudflare clearance cookie(外接 FlareSolverr)预留 `cookies` 注入位。
- **有界循环/分页原语**:统一 `nextPage` 规则 + 硬上限 `maxPages`,用于「多页正文」「多页目录」「加载更多」。**以"`nextPage` 为空即停 + maxPages 封顶"取代易死循环的 `start/end` 计数**。
- **黄金样例 + 验证回路**:书源携带 `samples`(样例 URL + 期望不变量,如 `minChapters`/`volumes`/`minContentChars`);引擎提供可执行断言,既做生成期校验、又做运行期 self-heal(失效自动退 `fallbacks`)。这是"让 AI 造书源"真正能用的闭环。
- 交付物含一份完整 **JSON Schema** 与 **bilixs 书源的 v2 示例**。

## Capabilities

### New Capabilities
- `book-source-rule-schema`: v2 结构化规则对象(via/select/extract/clean + anyOf/allOf/const/template + fallbacks)及其求值语义;不含任何紧凑字符串 DSL。
- `book-source-requests`: 结构化请求与 Cookie——headers / cookies / method / body / charset / retry / rateLimit。
- `book-source-loops`: 有界循环/分页原语(nextPage + maxPages),用于正文与目录,安全可终止。
- `book-source-samples`: 黄金样例与可执行不变量,驱动生成期校验与运行期自愈。

### Modified Capabilities
<!-- openspec/specs/ 为空;且本 change 明确不保留旧格式,故无既有 spec 的兼容性修改需登记。 -->

## Impact

- **代码**(`crates/parse-book-source`,规则层基本重写):
  - 新增 `Rule` 枚举(serde 表达 `Leaf{via,select,extract,clean} | AnyOf | AllOf | Const | Template`)及求值器(直接遍历执行,无字符串解析)。
  - `RuleSearch`/`RuleBookInfo`/`RuleToc`/`RuleContent` 字段类型由 `String` 改为 `Rule`。
  - **删除** `analyzer/analyzer_manager.rs` 的 DSL 解析(`split_rule_resolve`/`SPLIT_RULE`/`put`/`get`/`{{}}`);保留 `html`/`json`/(可选)`xpath` 的底层取值原语作为 `via` 后端。
  - 扩展 `HttpConfig` → 结构化 `http`(cookies / method / body / charset / retry)。
  - `get_content`/`get_chapters` 改用统一有界 `nextPage`+`maxPages` 循环;移除 `RuleContent::More` 的 `start/end` 计数。
  - 新增 `samples` 与校验模块(可执行不变量 + 失败反馈结构)。
- **格式/数据**:v2 书源是纯 JSON;旧 `bilixs.json`(紧凑格式)将被 v2 版替代。
- **依赖**:无新增强制依赖(复用 scraper / jsonpath-rust)。XPath(skyscraper)、反爬取页(wreq / FlareSolverr)是**正交的独立 change**,本 change 只把 `via:"xpath"` 与 `http.cookies` 的位置留好。
- **取舍**:放弃 Legado 书源生态导入,换取格式整洁、AI 友好、解析层更健壮;未来如需吃 Legado 源,可另写一个一次性 `legado→v2` 离线转换工具(不进运行时)。
