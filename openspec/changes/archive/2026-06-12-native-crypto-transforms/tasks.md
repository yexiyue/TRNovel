## 1. 依赖

- [x] 1.1 在 `crates/parse-book-source/Cargo.toml` 添加纯 Rust crypto/encode/hash 依赖(`aes` `cbc` `cfb-mode`(CFB-128) `ecb` `cipher`(含 block-padding/alloc feature) `aes-gcm` `des` `base64` `hex` `percent-encoding` `md-5` `sha1` `sha2` `hmac`),Cargo.toml 注释钉死 cipher 0.4 / digest 0.10 代际原因
- [x] 1.2 添加繁简依赖 `ferrous-opencc`;若不可用则退 `fast2s`/`character_converter`,在 design 注明实际选型
- [x] 1.3 `cargo tree` 确认零新增 C 依赖

## 2. 配置类型(source.rs)

- [x] 2.1 定义 `Codec`(base64|base64url|hex|url)枚举,派生 serde + JsonSchema
- [x] 2.2 定义 `HashStep { algo, output, hmacKey?, hmacKeyEnc? }`
- [x] 2.3 定义 `CipherStep { algo, mode, padding, op, key, keyEnc, iv?, ivEnc?, inputEnc, outputEnc }`(默认值贴合 decrypt 主场景)
- [x] 2.4 定义 `CnConvert`(t2s|s2t)枚举
- [x] 2.5 `CleanStep` 新增 `decode`/`encode`/`hash`/`cipher`/`cn` 字段(`Option`,`#[serde(default, skip_serializing_if = "Option::is_none")]`,派生 JsonSchema)

## 3. 错误与 transform 子模块

- [x] 3.1 `error.rs` 新增 `EvalError::Codec(String)` / `EvalError::Crypto(String)`
- [x] 3.2 新建 transform/crypto 子模块:`encode/decode`(base64/hex/url)纯函数 + 单测
- [x] 3.3 hash/hmac 纯函数(md5/sha1/sha256/sha512 + output hex|base64)+ 单测
- [x] 3.4 cipher 纯函数:AES CBC/ECB/CFB/GCM、DES/3DES,padding pkcs7/zero/none,key/iv 多编码,encrypt/decrypt + 单测(含已知向量)
- [x] 3.5 繁简 t2s/s2t 纯函数 + 单测

## 4. 接入 clean 流水线(eval.rs)

- [x] 4.1 `apply_clean` 改签名 `-> Result<String, EvalError>`,按 D2 固定顺序追加 decode→encode→hash→cipher→cn
- [x] 4.2 `eval_value`(eval.rs:43)调用处加 `?`
- [x] 4.3 现有 clean 单测改为 `unwrap()`/`?`,确认 regex/trim/prepend/append 行为不变

## 5. schema 防漂移

- [x] 5.1 重新生成 `crates/parse-book-source/book-source.schema.json`(`cargo run -p parse-book-source --features schema --example gen_schema`)
- [x] 5.2 `schema_is_in_sync` golden 测试通过(`--features schema`)

## 6. 体积评估与可选 feature

- [x] 6.1 测量加入繁简词典后的体积增量:opencc 内嵌词典约 3MB,对独立分发的 TUI 可接受
- [x] 6.2 决策:体积可接受,**保持 `cn` 默认开启、不 feature-gate**(若日后体积敏感再按本条收进 `cn-convert` feature)

## 7. 端到端与门禁

- [x] 7.1 集成测试:一条 content 规则 `clean: [{decode: base64},{cipher:{...aes-cbc decrypt...}},{trim:true}]` 端到端解出明文(离线 MockFetcher)
- [x] 7.2 `cargo test -p parse-book-source --all-features` 全绿
- [x] 7.3 `cargo clippy --all-targets --all-features --workspace -- -D warnings` 与 `cargo fmt --all --check`

## 8. 文档

- [x] 8.1 在 `docs/` 书源参考补充 `clean` 的 decode/encode/hash/cipher/cn 用法与一个 AES 解密正文示例
