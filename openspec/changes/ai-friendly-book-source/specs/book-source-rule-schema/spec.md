## ADDED Requirements

### Requirement: 结构化叶子规则求值

系统 SHALL 以结构化对象表达单条抽取规则,字段为 `via`(枚举 css|xpath|json|regex|raw)、`select`、可选 `index`、`extract`(枚举 text|ownText|html|innerHtml|outerHtml 或 `{attr}`)、可选 `clean`(有序后处理流水线)。系统 MUST NOT 使用任何紧凑字符串 DSL(无 `@css:`/`##`/`&&`/`||`/`{{}}`/`@put`/`@get`)。求值按 `via` 路由到对应抽取后端。

#### Scenario: CSS 取文本

- **WHEN** 规则为 `{ "via": "css", "select": ".module-item-title", "extract": "text" }`,上下文为含该元素的 HTML
- **THEN** 返回该元素的文本内容

#### Scenario: 取属性并清洗

- **WHEN** 规则为 `{ "via": "css", "select": "img", "extract": {"attr":"data-src"}, "clean":[{"trim":true}] }`
- **THEN** 返回 `img` 的 `data-src` 属性值并去除首尾空白

### Requirement: 规则组合子

系统 SHALL 支持组合子:`firstOf`(返回首个非空子规则结果,即回退/自愈)、`concat`(拼接非空子规则结果,可带 `join`)、`literal`(字面量)、`template`(插值 `{{key}}`/`{{page}}`/`{{pageSize}}` 及请求级命名变量)。组合子键特意不与 JSON-Schema 关键字(anyOf/allOf/const)同名;按其唯一键判别,与叶子规则互斥,可被 JSON-Schema 的 `oneOf` 无歧义描述。

#### Scenario: firstOf 回退取首个非空

- **WHEN** 规则为 `{ "firstOf": [ {"via":"css","select":".module-row-title","extract":"text"}, {"via":"css","select":"h2","extract":"text"} ] }`,上下文是只含 `h2` 的卷标题节点
- **THEN** 第一个子规则为空、回退到第二个,返回 `h2` 文本

#### Scenario: template 插值分页 URL

- **WHEN** 规则为 `{ "template": "{{base}}/book/list_{{page}}.html" }`,变量 `base`/`page` 已就绪
- **THEN** 返回完成插值后的 URL

### Requirement: 列表规则与值规则

字段角色 SHALL 决定规则产出"一个值"还是"一组元素":`search.list`/`explore.list`/`toc.list` 为列表规则(MUST 选中所有匹配,每个匹配成为后续 item 规则的上下文);其余字段为值规则。规则形状统一,语义由其所在字段位置决定。

#### Scenario: 列表规则产出逐项上下文

- **WHEN** `toc.list` 选中目录页中所有章节/卷节点
- **THEN** 引擎对每个节点分别用 `toc.name`/`toc.url`/`toc.isVolume` 求值,产出有序条目

### Requirement: 规则可被 JSON-Schema 完整描述

整个 v2 书源(含 `Rule`)MUST 有一份 JSON-Schema,使「约束解码 / 结构化输出」能在生成期保证形状合法(`via`/`extract` 受 enum 约束、`clean.regex` 可受 pattern 约束)。

#### Scenario: 非法 via 被拒

- **WHEN** 某规则 `via` 取了枚举外的值
- **THEN** 按 Schema 校验该书源时报告该字段非法
