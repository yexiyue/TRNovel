## Context

`xpath-backend` 补齐了取值后端,`native-crypto-transforms` 把确定性加解密做成了 `clean` 算子并把 `apply_clean` 升级为 `Result`。剩下唯一表达不了的是**运行时逻辑编排**(控制流)。本 change 加一个受控的 JS 逃生舱。

设计基调:**克制 + 单一真相源**。格式仍以结构化为主,JS 只做逻辑编排;JS 里可用的加解密助手叫 `crypto`(中性命名,非 `java`),其实现复用 ②、不另起炉灶。

## Goals / Non-Goals

**Goals**
- `Rule::Js` 值规则 + `clean` 的 `js` 步,支持条件/循环/字符串处理。
- JS 上下文注入只读的 `result`/`baseUrl`/变量,以及 `crypto` 加解密助手对象(后端复用 ②)。
- `js` cargo feature 门控,默认关闭;关闭时求值返回 `Unsupported("js")`,但配置仍可解析(变体/schema 恒存在)。
- 同步求值(无 async/网络),适配现有同步 `eval_value`/`apply_clean`。

**Non-Goals**
- `java.*` 命名(用中性 `crypto`)及其网络/文件/webView/UI 桶(有副作用、依赖宿主)。
- Legado JS 语法自动转换(转换 skill)。
- `Rule::Js` 作为列表规则产出数组(v1 仅值规则;列表退化为单值)。
- crypto 的第二套实现(JS `crypto.*` 只薄绑定 ② 的 fn)。

## Decisions

### D1 — JS 引擎选型:纯 Rust 优先(`boa_engine`),`rquickjs` 为备选
- **`boa_engine`(纯 Rust)** ✅ 首选:与本项目"零 C 依赖"原则一致(承 xpath/opencc 同一取舍逻辑)。我们的 JS 用量是"胶水"(字符串处理 + 简单控制流),boa 足以胜任,且无 C 工具链/交叉编译风险。
- **`rquickjs`(QuickJS,C)** 备选:更快、与 Legado(QuickJS 系)JS 兼容性更好;但 bundled QuickJS 是 C 源码、需 C 工具链编译。**仅当实测 boa 在目标 JS 上不足时再换**,并把它收进同一 `js` feature(对外语义不变)。
- 决策在 `js` feature 内封装,引擎类型不外泄,后续可替换。

### D2 — `js` feature 门控,默认关闭
JS 引擎是重依赖(编译时长 + 体积)。因我们书源正常不写 JS,把引擎收进 `#[cfg(feature = "js")]`:
- **AST 与 schema 恒含** `Rule::Js` / `CleanStep.js`(配置可移植、可被未开 feature 的工具解析与校验)。
- feature **关**:`eval_value`/`apply_clean` 遇 JS → `Err(EvalError::Unsupported("js"))`。
- feature **开**:真正求值。app 显式启用(`Cargo.toml` 已有 `browser` 先例)。

### D3 — `Rule::Js` 在 untagged 枚举里的位置
`Rule` 是 `#[serde(untagged)]`,按变体顺序匹配,`Leaf`(全可选字段)兜底。`Js { js: String }` 有唯一必填键 `js`,**必须置于 `Leaf` 之前**,否则会被 `Leaf` 吞掉。顺序:FirstOf → Concat → Literal → Template → **Js** → Leaf。

### D4 — JS 执行模型(同步、无副作用)
- 输入:当前上下文字符串作为全局 `result`(对齐 Legado `result` 约定);注入 `baseUrl` 及 `vars`(`key`/`page`/`pageSize`/`base` 等)为全局只读。
- 输出:脚本最后一个表达式的值 / 显式返回值,转为字符串作为规则结果(非字符串按 JS `String()` 语义)。
- 同步:无网络/无 IO,boa 同步求值,直接嵌入现有同步 `eval_value`/`apply_clean`,**不引入 async**。
- 沙箱:除注入的只读字符串与纯计算 `crypto` 助手外,不暴露任何 IO/网络/宿主能力 —— 攻击面极小。

### D5 — 注入 `crypto` 助手对象(中性命名,薄绑定 ②)
JS 里能直接做加解密,覆盖"crypto 嵌在控制流里"的场景(如 `crypto.md5(运行时时间戳 + salt)`——时间戳在 JS 算出,无法预先拆成结构化步)。关键约束:
- **命名 `crypto`,不是 `java`**:中性、clean-room;`java.*` 留给原样跑 Legado JS 的转换 skill,不渗进引擎。与"书源不必带 `java`"定位一致。
- **单一真相源**:`crypto.{base64Decode/base64Encode/hexDecode/hexEncode/md5/sha1/sha256/sha512/hmac/aesDecode/aesEncode/desDecode/.../t2s/s2t}` 的实现**就是调用 ② 已写好且已测的纯 Rust fn**,不另写一套。同一能力两个入口(结构化 `cipher`/`hash`/`decode` 步 + JS `crypto.*`),但实现唯一。
- **结构化仍优先**:能用 `concat(模板, 模板+hash 步)` 等结构化表达的签名,推荐走结构化;`crypto.*` 只在 crypto 必须与控制流交织时用。
- 绑定签名以"易用 + 贴近 ② 的 fn 形参"为准,不追求与 Legado `java.*` 逐方法全等(转换器再做映射)。

### D6 — 错误语义
- 脚本抛错/语法错 → `EvalError::Js(String)`,沿 `eval_value` 上抛。
- feature 关 → `EvalError::Unsupported("js")`。
- 与 crypto 一致:JS 失败是配置错误,显式报错而非静默空(便于书源作者定位;doctor/verify 兜诊断)。

## Risks / Trade-offs
- **boa 能力/性能**:对纯逻辑胶水足够;不足则换 rquickjs(D1),feature 封装使切换不破坏对外语义。
- **编译时长/体积**:故默认关闭 feature(D2);开启者(app)承担。
- **JS 安全**:仅注入只读字符串 + 纯计算 `crypto` 助手,无任何 IO/网络出口,攻击面极小。

## Migration Plan
纯新增。重新生成 `book-source.schema.json`(Rule +Js、CleanStep +js)并更新 golden。旧书源零影响;未开 feature 的构建仍能解析含 Js 的书源(求值到才报 Unsupported)。

## Open Questions
- boa 是否需要为某些常用全局(如 `JSON`/`encodeURIComponent`)补垫片——实现期按测试用例补。
