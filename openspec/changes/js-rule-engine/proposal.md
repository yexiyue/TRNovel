## Why

绝大多数书源用「结构化抽取 + 确定性 crypto transform」(见 `xpath-backend` / `native-crypto-transforms`)即可拿下。但仍有一小撮源需要**逻辑编排**——条件、循环、字符串切片、把多个取值结果按运行时逻辑拼起来,有时还要在控制流里嵌加解密(如 `crypto.md5(运行时时间戳 + salt)` 这种没法预先拆成结构化步的签名)。这类**控制流**无法用声明式结构表达,需要一个 JS 逃生舱。

**定位(关键):我们的书源 schema 仍以结构化为主,作者(AI 或人)正常不必写 JS。** `Rule::Js` 只是**少数动态场景的逃生舱**,同时也是未来 `legado→v2` 转换 skill 的落点(Legado 的 `<js>` 块转成我们的 `Rule::Js`)。

**命名:JS 里的加解密助手叫 `crypto`,不叫 `java`。** crypto 实现是 `native-crypto-transforms` 的纯 Rust fn(单一真相源);JS 里的 `crypto` 对象只是这批 fn 的**薄绑定**——同一能力两个入口(结构化步 + JS `crypto.*`),但**实现唯一**。用中性的 `crypto` 命名而非 Legado 的 `java`,既符合"书源不必带 `java`"的定位,也让引擎保持 clean-room。`java.*` 命名只在原样跑 Legado JS 时才有意义,那是转换 skill 的职责。

**范围克制**:只做"纯逻辑编排 + `crypto` 加解密助手"。**不复刻** Legado `java.*` 的网络/文件/webView/UI 桶(有副作用、依赖宿主)。

## What Changes

- **新增 `Rule::Js { js: String }` 变体**(值规则):以当前上下文为 `result`、注入站点/页码变量,执行 JS 脚本,返回字符串。位于 `Leaf` 之前(`js` 为唯一判别键)。
- **`CleanStep` 新增 `js: Option<String>`**:在 clean 流水线里对当前字符串跑一段 JS 后处理(承 `native-crypto-transforms` 已把 `apply_clean` 升级为 `Result`)。
- **注入绑定**:只读 `result`(当前上下文)、`baseUrl`、`key`/`page`/`pageSize`/`base` 变量,以及 **`crypto` 对象**——`crypto.{base64Decode/base64Encode/hexDecode/hexEncode/md5/sha1/sha256/sha512/hmac/aesDecode/aesEncode/desDecode/.../t2s/s2t}`,**后端就是 ② 的纯 Rust fn**(零重复实现)。命名为 `crypto`,不是 `java`。
- **`js` cargo feature(默认关闭)**:JS 引擎是重依赖且非基线必需。`Rule::Js` / `CleanStep.js` 变体与 schema **恒存在**(配置可移植);**未启用 `js` feature 时,求值返回 `EvalError::Unsupported("js")`**(镜像 xpath/cn 模式)。app 显式开启(如已开 `browser`)。
- **重新生成 `book-source.schema.json`**(Rule 多 `js` 变体、CleanStep 多 `js` 字段),golden 测试更新。

## Capabilities

### New Capabilities
- `book-source-js`: JS 逻辑编排逃生舱——`Rule::Js` 值规则与 `clean` 的 `js` 步;注入 `result`/`baseUrl`/变量 + 复用 ② 原生 crypto 的 `crypto` 对象(中性命名,非 `java`);`js` feature 门控,关闭时返回 Unsupported。

## Impact

- **代码**(`crates/parse-book-source`):
  - `src/source.rs`:`Rule` 新增 `Js { js }`(置于 `Leaf` 前);`CleanStep` 新增 `js: Option<String>`;均派生 JsonSchema。
  - `src/eval.rs`:`eval_value` 处理 `Rule::Js`;`apply_clean` 处理 `js` 步;注入变量与 `crypto` 对象;`eval_list` 中 `Js` 退化为单值。
  - 新增 `js` 子模块(`#[cfg(feature = "js")]`):引擎封装 + `crypto` 对象绑定(调用 ② 的 crypto fn)。
  - `src/error.rs`:新增 `EvalError::Js(String)`(脚本错误);沿用 `Unsupported("js")`(feature 关)。
  - `examples/gen_schema.rs` 重新生成 schema;golden 测试更新。
  - `Cargo.toml`:`js` feature + JS 引擎依赖(纯 Rust 优先,见 design D1)。
- **app**(`/Volumes/yexiyue/TRNovel/Cargo.toml`):`parse-book-source` 启用 `js` feature。
- **兼容性**:纯新增。旧书源不含 `js` → 行为不变。未开 `js` feature 的构建仍可解析含 `Rule::Js` 的书源,仅在求值到该规则时报 `Unsupported`。
- **非目标**:Legado `java.*` 的网络/文件/webView/UI 桶;Legado JS 字符串语法/`<js>` 块的自动转换(独立 `legado→v2` 转换 skill);让 JS 成为书源主流写法(结构化仍是首选)。
