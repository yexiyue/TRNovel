## 1. feature 与依赖

- [x] 1.1 `crates/parse-book-source/Cargo.toml` 新增 `js` feature(默认关闭)+ JS 引擎可选依赖(`boa_engine` 纯 Rust 优先;不足则 `rquickjs`,见 design D1)
- [x] 1.2 `cargo tree --features js` 评估引入项;确认 boa 无 C 依赖
- [x] 1.3 `/Volumes/yexiyue/TRNovel/Cargo.toml` 给 `parse-book-source` 启用 `js` feature

## 2. AST 与 schema(恒存在)

- [x] 2.1 `source.rs`:`Rule` 新增 `Js { js: String }`,置于 `Leaf` 之前(唯一键 `js`)
- [x] 2.2 `source.rs`:`CleanStep` 新增 `js: Option<String>`(`#[serde(default, skip_serializing_if = "Option::is_none")]`,派生 JsonSchema)
- [x] 2.3 serde 单测:含 `js` 变体 / `clean.js` 的书源可解析、可 round-trip

## 3. JS 引擎封装与 crypto 绑定(#[cfg(feature = "js")])

- [x] 3.1 新建 `js` 子模块:封装引擎,提供 `eval_js(script, result, vars) -> Result<String, EvalError>`
- [x] 3.2 注入只读全局:`result`、`baseUrl`、`vars`(key/page/pageSize/base)
- [x] 3.3 注入 **`crypto` 对象**(中性命名,非 `java`):base64/hex 编解码、md5/sha/hmac、aes/des 加解密、t2s/s2t —— **后端调用 `native-crypto-transforms` 的纯 Rust fn**(零重复实现,见 design D5)
- [x] 3.4 脚本返回值 → 字符串(JS `String()` 语义);脚本错误 → `EvalError::Js`
- [x] 3.5 必要时补 JS 全局垫片(`JSON`/`encodeURIComponent` 等,按测试用例)

## 4. 接入求值器(eval.rs)

- [x] 4.1 `eval_value` 处理 `Rule::Js`:feature 开 → `eval_js(...)`;关 → `Err(Unsupported("js"))`
- [x] 4.2 `apply_clean` 处理 `js` 步:同上(承 ② 的 `Result` 签名)
- [x] 4.3 `eval_list` 中 `Rule::Js` 退化为单值(非空入列)
- [x] 4.4 `error.rs` 新增 `EvalError::Js(String)`

## 5. schema 防漂移

- [x] 5.1 重新生成 `crates/parse-book-source/book-source.schema.json`(`--features schema`,需同时含 js 变体——按需 `--features "schema js"`)
- [x] 5.2 `schema_is_in_sync` golden 测试通过

## 6. 测试与门禁

- [x] 6.1 单测(feature 开):`Rule::Js` 用 `result` + 变量拼出 URL/签名
- [x] 6.2 单测(feature 开):`clean.js` / `crypto.aesEncrypt`+`crypto.aesDecrypt` 往返解出明文(与 ② cipher 步同后端)
- [x] 6.3 单测(feature 关):求值到 `Rule::Js` 返回 `Unsupported("js")`,但书源仍能解析
- [x] 6.4 `cargo test -p parse-book-source --all-features` 全绿;另跑一次**不带 js** 的默认 feature 测试确认门控
- [x] 6.5 `cargo clippy --all-targets --all-features --workspace -- -D warnings` 与 `cargo fmt --all --check`

## 7. 文档

- [ ] 7.1 `docs/` 书源参考补充:`Rule::Js` / `clean.js` 用法、`result`/`baseUrl`/`crypto` 可用项、"结构化优先、JS 仅逃生舱"的定位,以及 `js` feature 说明
