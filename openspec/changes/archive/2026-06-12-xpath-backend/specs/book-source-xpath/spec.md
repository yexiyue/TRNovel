## ADDED Requirements

### Requirement: XPath 抽取后端

抽取后端 SHALL 支持 `via: "xpath"`。`extract`(值规则)与 `select_all`(列表规则)MUST 对 `Via::Xpath` 提供真实实现,不再返回 `Unsupported`。实现 MUST 为纯 Rust、零 C 依赖。

#### Scenario: XPath 选中元素并按 extract 取值

- **WHEN** 一条叶子规则为 `{ "via": "xpath", "select": "//div[@class='title']", "extract": "text" }`,作用于含该元素的 HTML
- **THEN** 返回该元素的文本(去首尾空白),语义与 `via: "css"` + `extract: text` 一致

#### Scenario: XPath 标量结果(属性/text())直接取值

- **WHEN** 一条叶子规则为 `{ "via": "xpath", "select": "//a/@href" }`(或 `.../text()`、`string(...)`、`concat(...)`)
- **THEN** 直接返回该标量字符串值,`extract` 字段被忽略

#### Scenario: XPath 列表规则产出子上下文

- **WHEN** 一条列表规则为 `{ "via": "xpath", "select": "//ul/li" }`,后续 item 规则在每个子上下文上继续求值
- **THEN** 每个匹配元素以其 outerHTML 作为子上下文返回,item 规则(可为 xpath 或 css)能在「列表项自身 + 其后代」上继续 self-or-descendant 求值

### Requirement: 脏 HTML 宽松解析与安全降级

XPath 后端 SHALL 宽松解析不良构的书源 HTML(未闭合标签、属性无引号、HTML5 void 元素)。当 HTML 无法桥接到 XPath 引擎、或 XPath 合法但无匹配时,后端 MUST 返回空结果(值规则空串、列表规则空 Vec)而非 panic 或上抛错误,以便 `firstOf` 回退链照常工作。仅当 XPath 表达式本身语法非法时,后端 MUST 返回求值错误。

#### Scenario: 脏 HTML 可被宽松解析后求值

- **WHEN** 输入 HTML 含未闭合标签 / 无引号属性 / `<br>` 等 void 元素,XPath 表达式合法且有匹配
- **THEN** 后端宽松解析后返回正确匹配,不因不良构而失败

#### Scenario: 合法但无匹配降级为空

- **WHEN** XPath 表达式语法合法但在文档中无匹配
- **THEN** 值规则返回空串、列表规则返回空 Vec,`firstOf` 可回退到下一条子规则

#### Scenario: 非法 XPath 表达式报错

- **WHEN** `select` 是语法非法的 XPath 表达式
- **THEN** 后端返回求值错误(而非静默空结果)
