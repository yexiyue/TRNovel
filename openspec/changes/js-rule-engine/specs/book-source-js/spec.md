## ADDED Requirements

### Requirement: JS 逻辑编排逃生舱

规则引擎 SHALL 支持 JS 逃生舱用于运行时逻辑编排。`Rule` MUST 新增 `Js { js }` 值规则变体(位于 `Leaf` 之前);`CleanStep` MUST 新增可选 `js` 步。求值时 MUST 注入只读全局 `result`(当前上下文)、`baseUrl` 与变量(`key`/`page`/`pageSize`/`base`),脚本返回值 MUST 转为字符串作为结果。`Rule::Js` 作为列表规则时 MUST 退化为单值。

#### Scenario: Js 规则用变量拼出签名 URL

- **WHEN** 一条规则为 `{ "js": "baseUrl + '/c?p=' + page + '&s=' + java.md5(page + 'salt')" }`,在 search/content 阶段求值
- **THEN** 返回脚本计算出的字符串(用到注入的 `baseUrl`/`page` 与 `java.md5`)

#### Scenario: clean 的 js 步后处理当前字符串

- **WHEN** clean 链含 `{ "js": "result.split('|')[1]" }`,`result` 为前序步骤产出的字符串
- **THEN** 返回脚本对 `result` 处理后的字符串

### Requirement: crypto 对象复用原生 crypto

启用 JS 时,JS 上下文 SHALL 注入名为 `crypto` 的助手对象(中性命名,MUST NOT 命名为 `java`),提供编解码/哈希/加解密/繁简方法(base64/hex、md5/sha/hmac、aes/des、t2s/s2t),其实现 MUST 复用 `native-crypto-transforms` 的纯 Rust 函数(单一真相源,不重复实现)。

#### Scenario: crypto 解密与结构化 cipher 结果一致

- **WHEN** 一段 JS 调 `crypto.base64Decode` + `crypto.aesDecode` 解密某密文,另一条规则用结构化 `cipher` 步以相同参数解密同一密文
- **THEN** 两者产出相同明文

### Requirement: js feature 门控与可解析性

JS 引擎 SHALL 由 `js` cargo feature 门控且默认关闭。`Rule::Js` / `CleanStep.js` 变体与生成的 `book-source.schema.json` MUST 恒存在,使含 JS 的书源在未启用 `js` 的构建下仍可被解析与 schema 校验。未启用 `js` 时,求值到 JS MUST 返回 `EvalError::Unsupported("js")`;脚本运行错误 MUST 返回 `EvalError::Js`。

#### Scenario: 未启用 js feature 仍可解析但求值报 Unsupported

- **WHEN** 一个含 `Rule::Js` 的书源在未启用 `js` feature 的构建中被解析,并求值到该规则
- **THEN** 书源解析成功,求值到该规则时返回 `EvalError::Unsupported("js")`

#### Scenario: 启用 js feature 后正常求值

- **WHEN** 同一书源在启用 `js` feature 的构建中求值到 `Rule::Js`
- **THEN** 脚本被执行并返回字符串结果
