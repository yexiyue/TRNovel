## Why

v2 引擎的抽取后端(`backend.rs`)已支持 `css` / `json` / `regex` / `raw`,但 `Via::Xpath` 是**占位未实现**——`extract`(backend.rs:29)与 `select_all`(backend.rs:61)两处都直接返回 `Err(EvalError::Unsupported("xpath"))`。

XPath 是 Legado 生态里仅次于 CSS 的主流取值方式,很多站点的层级/轴选择(祖先、兄弟、按位置)用 CSS 表达不了或很别扭。补全 XPath 直接抬高书源覆盖面,也为未来的 `legado→v2` 转换器扫清一类规则。

**约束:纯 Rust、零 C 依赖。** 本项目通过 cargo-dist 跨平台打包(参见 CLAUDE.md 的 `msvc-crt-static=false` / onnxruntime CRT 教训),`libxml`(libxml2 绑定)这类带 C 依赖的方案一律排除。

## What Changes

- **实现 `Via::Xpath` 抽取后端**:新增 `xpath_extract()` 与 `xpath_select_all()` 两个函数,镜像现有 `html_extract` / `json_extract` / `regex_extract` 的模式,接入 `extract` / `select_all` 的策略分发(backend.rs:29、:61)。**不改动 `Rule` AST / `eval.rs` / `engine.rs`**——开闭原则,引擎对后端无意识。
- **脏 HTML 宽松解析桥**:书源 HTML 常不良构(未闭合标签、属性无引号)。复用已有的宽松 HTML 解析栈解析 DOM,再桥接到 XPath 引擎求值,避免 XML-strict 解析器直接拒收。
- **XPath 求值语义对齐我们的取值模型**:`select`(XPath 表达式)产出可能是节点集(元素/属性/文本)或标量(string/number/boolean):
  - 标量结果(如 `//a/@href`、`string(...)`、`concat(...)`)直接作为值,`extract` 被忽略;
  - 元素节点集:按 `index` 取节点后,套用 `extract`(text/ownText/html/innerHtml/outerHtml/{attr}),与 CSS 后端一致;
  - `select_all`(列表规则)返回每个匹配节点的 outerHTML 子上下文串,供后续 item 规则继续求值(与 CSS 后端 self-or-descendant 行为对齐)。
- **解析失败安全降级**:HTML 无法桥接/XPath 非法匹配为空时返回空串(而非 panic),让 `firstOf` 组合子的回退/自愈链照常工作;仅 XPath 表达式本身语法非法时报 `EvalError`。

## Capabilities

### New Capabilities
- `book-source-xpath`: `Via::Xpath` 抽取后端——宽松 HTML 解析 + XPath 1.0 求值,语义对齐既有取值模型(select/index/extract/select_all),纯 Rust 无 C 依赖。

## Impact

- **代码**(`crates/parse-book-source`):
  - `src/backend.rs`:替换两处 `Err(Unsupported)` 占位为真实实现;新增 `xpath_extract` / `xpath_select_all` 及宽松 HTML→XPath 桥接辅助函数。
  - 单元测试:覆盖元素节点 + `extract`、`/@attr` 标量、`/text()` 标量、`select_all` 列表子上下文、脏 HTML 宽松解析、空匹配降级、非法表达式报错。
- **依赖**:新增纯 Rust XPath crate(`sxd-document` + `sxd-xpath`);宽松解析复用现有 `dom_query`/`html5ever` 栈,**不新增 C 依赖**。
- **兼容性**:纯新增能力。既有 css/json/regex/raw 书源行为不变;`Via` 枚举已含 `Xpath` 变体,无 schema 变更(`book-source.schema.json` 不变)。
- **非目标**:XPath 2.0/3.0 函数;`legado→v2` 转换器(独立未来 skill);用 XPath 替代 CSS(二者并存,XPath 作为补充)。
