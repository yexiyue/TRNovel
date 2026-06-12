## Why

研究表明:Legado 的 `java.*` 工具对象里**约 43 个方法本质是确定性加解密/编码/哈希纯函数**(`aes*`/`des*`/`md5*`/`sha*`/`base64*`/`hex*`/`encodeURI`/`t2s/s2t` 等),占其表面的一半以上,且**全部可用纯 Rust、零 C 依赖实现**。

很多书源的反爬只是"AES 解密正文 + Base64/Hex 解码 + MD5 签名 URL"——这是**没有控制流的确定性 reduce**,根本不需要 JS 引擎。把这些做成 `clean` 流水线里的结构化 transform 步,即可**在完全不引入 JS 的前提下覆盖一大类反爬书源**。

这一步还是 JS 引擎(后续 `js-rule-engine` change)的去风险前置:等 JS 落地时,`java.aesDecode/base64Decode/md5Encode` 等直接绑定到这里已写好且测过的 Rust fn,JS 层只剩逻辑编排。

**约束:纯 Rust、零 C 依赖**(cargo-dist 跨平台打包;参见 CLAUDE.md 的 onnxruntime CRT 教训)。**我们的书源 schema 仍保持纯结构化——不暴露 `java.*`/JS 风格语法**;Legado 的 `java.*` 兼容性由未来独立的 `legado→v2` 转换 skill 负责。

## What Changes

- **扩展 `CleanStep`**,在现有 `regex`/`replace`/`trim`/`prepend`/`append` 之后新增确定性 transform 字段(均 `Option`,后向兼容):
  - `decode` / `encode`:`base64` | `base64url` | `hex` | `url`(URL 百分号编解码)。
  - `hash`:`{ algo: md5|sha1|sha256|sha512, output: hex|base64, hmacKey?, hmacKeyEnc? }`(含 HMAC)。
  - `cipher`:`{ algo: aes|des|tripleDes, mode: cbc|ecb|cfb|gcm, padding: pkcs7|zero|none, op: decrypt|encrypt, key, keyEnc, iv?, ivEnc?, inputEnc, outputEnc }`——覆盖 Legado 常见 key/iv 编码组合。
  - `cn`:`t2s`(繁→简) | `s2t`(简→繁)。
- **`apply_clean` 改签名为 `-> Result<String, EvalError>`**(解码/解密会失败),`eval_value`(eval.rs:43)调用处加 `?`。新增 `EvalError::Crypto` / `EvalError::Codec` 变体。
- **同一组纯 Rust crypto fn 内聚为子模块**(如 `backend::crypto` / `transform`),供 `apply_clean` 调用,并为后续 JS 引擎的 `java.*` 后端预留复用入口。
- **重新生成 `book-source.schema.json`**(新增字段),golden 防漂移测试随之更新。

## Capabilities

### New Capabilities
- `book-source-crypto-transforms`: `clean` 流水线的确定性加解密/编码/哈希/繁简 transform 步(AES/DES/3DES、base64/hex/url、md5/sha/hmac、t2s/s2t),纯 Rust 无 C 依赖;`apply_clean` 升级为可失败。

## Impact

- **代码**(`crates/parse-book-source`):
  - `src/source.rs`:`CleanStep` 新增 `decode`/`encode`/`hash`/`cipher`/`cn` 字段及其类型(`Codec`/`HashStep`/`CipherStep`/`CnConvert`),全部 `#[serde(default, skip_serializing_if)]` + `JsonSchema` 派生。
  - `src/eval.rs`:`apply_clean` → `Result`;按固定步内顺序追加新算子;`eval_value` 调用处加 `?`。
  - 新增 crypto/transform 子模块(纯函数 + 单测)。
  - `src/error.rs`:新增 `EvalError::Crypto` / `EvalError::Codec`。
  - `examples/gen_schema.rs` 重新生成 `book-source.schema.json`;`schema_is_in_sync` golden 测试更新。
- **依赖**(全部纯 Rust,**钉在 cipher 0.4 / digest 0.10 代际**以共享 trait):`aes` `cbc` `cfb8` `ecb` `block-padding` `cipher` `aes-gcm`、`des`、`base64` `hex` `percent-encoding`、`md-5` `sha1` `sha2` `hmac`、繁简用 `ferrous-opencc`(**禁用 `opencc-rust`——带 C 依赖**)。
- **兼容性**:纯新增字段,旧书源(不含新字段)反序列化为默认 `None`,行为不变。`apply_clean` 改 `Result` 是 crate 内部改动,不影响公开 `Engine` API。
- **非目标**:RSA / 非对称(留待 `js-rule-engine` 的 `java.*` 后端或后续 change);任何需 JS 控制流的编排;在我们 schema 暴露 `java.*` 语法;Legado 书源导入(独立转换 skill)。
