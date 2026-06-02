## Context

`backend.rs` 是 Strategy 模式的抽取后端层:`extract`(取一个值)与 `select_all`(取列表子上下文)按 `Via` 静态分派。`Via::Xpath` 变体已在 `source.rs` 定义,但后端两处均占位 `Err(Unsupported)`。本 change 只在后端落地 XPath,不触碰上层 AST/求值器/引擎。

关键张力:**书源 HTML 是脏的**(未闭合标签、属性无引号、HTML5 void 元素),而成熟的纯 Rust XPath 引擎(`sxd-xpath`)基于 **XML-strict** 解析器(`sxd-document`),直接喂脏 HTML 会解析失败。必须有一座"宽松进、规整出"的桥。

## Goals / Non-Goals

**Goals**
- `Via::Xpath` 可用于 search/explore/bookInfo/toc/content 的任意规则字段。
- 取值语义与 CSS 后端对齐(select+index+extract;select_all 产出 outerHTML 子上下文)。
- 纯 Rust,零 C 依赖。
- 脏 HTML 不致命:解析不了就返回空,交 `firstOf` 兜底。

**Non-Goals**
- XPath 2.0+。`sxd-xpath` 实现 XPath 1.0,足够书源使用。
- Legado xpath 规则的字符串语法兼容(`//a/@href` 这类表达式本身通用,但 Legado 的 `@XPath:` 前缀/混合管线属于转换器范畴)。

## Decisions

### D1 — XPath 引擎选 `sxd-xpath` + `sxd-document`(纯 Rust)
候选对比:

| 方案 | XPath | 脏 HTML | C 依赖 | 取舍 |
|---|---|---|---|---|
| `libxml`(libxml2) | 完整 + HTML 宽松 | ✅ | ❌ 有 C 依赖 | 排除(违反项目原则) |
| `sxd-document`+`sxd-xpath` | XPath 1.0 完整 | ❌ XML-strict | ✅ 纯 Rust | **选它 + 自建宽松桥** |
| `skyscraper` / `amxml` | 弱/XML-only | — | ✅ | XPath 支持不足 |

### D2 — 宽松 HTML → XPath 桥:复用现有解析器 + 序列化为良构 XML
复用已是依赖的 `dom_query`(基于 html5ever)宽松解析脏 HTML 得到规范化 DOM,再序列化为良构 XML 文本喂给 `sxd-document::parser::parse`。html5ever 解析阶段已自动补全未闭合标签、修正嵌套,序列化输出趋于良构。

实现细节与防御:
- 若 `sxd` 仍解析失败(极端畸形),`xpath_*` 返回空串(`extract`)/空 Vec(`select_all`),不 panic、不上抛——`firstOf` 回退链照常生效。
- HTML5 void 元素(`<br>`/`<img>` 等)需以自闭合形式输出,确保 XML 良构;若所选序列化路径不满足,改为遍历 DOM 树直接构建 `sxd` 文档(更稳,无序列化往返脆弱性)。最终以"真实脏书源能跑通"为验收口径,实现时择优。

### D3 — XPath 结果 → 字符串的语义映射
`sxd_xpath::Value` 四种:
- `String` / `Number` / `Boolean` → 直接 `to_string`(`String` 去首尾空白);`extract` 忽略(表达式已自带取值意图,如 `//a/@href`、`concat(...)`)。
- `Nodeset`:按文档序取节点;`xpath_extract` 用 `resolve_index` 选一个,`xpath_select_all` 全取。
  - 元素节点 → 套用 `Extract`:`Text`(递归子文本 trim)/`OwnText`(直接子文本)/`Html`(内层转可读文本,复用 `clean_html`)/`InnerHtml` / `OuterHtml` / `Attr{attr}`。
  - 属性节点 → 取属性值(trim);文本节点 → 取文本(trim)。
- 空节点集 / 空标量 → 空串(`extract`)或空 Vec(`select_all`)。

### D4 — `select_all` 的子上下文 = outerHTML(对齐 CSS)
CSS 后端 `select_all` 返回每个匹配节点的 `n.html()`(outerHTML)作为子上下文,使后续 item 规则能在"列表项自身 + 其后代"上继续 self-or-descendant 求值。XPath 后端对元素节点集采用同一约定(序列化各匹配元素为 outerHTML);属性/文本节点集则直接收集其字符串值。

### D5 — `index` 复用 `resolve_index`
直接复用 backend.rs 既有 `resolve_index`(None→0、负数从末尾、越界回退首/末),与其它后端一致。

## Risks / Trade-offs

- **极端脏 HTML 桥接失败** → 返回空,靠 `firstOf` 兜底;文档注明 XPath 对良构度的要求高于 CSS,推荐脏站优先用 CSS。
- **序列化往返性能**:每次 XPath 求值都解析+序列化+再解析一遍。toc/content 分页场景下每页一次,可接受;如成热点再缓存。
- **XPath 1.0 局限**:不支持 2.0 的正则/序列函数;书源极少用到,可接受。

## Migration Plan
纯新增,无迁移。`Via` 枚举与 schema 不变,旧书源零影响。

## Open Questions
- 桥接实现走"序列化往返"还是"遍历建树"——以实现期真实脏书源验收择优,不影响对外语义。
