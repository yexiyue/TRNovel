## Context

`clean` 是叶子规则取值后的有序后处理流水线(eval.rs:`apply_clean`,当前 `-> String`,做 regex→trim→prepend→append)。书源反爬的"解密正文/解码/签名"本质是确定性纯函数,天然适配 transform 步。本 change 把这批纯 Rust crypto/encode/hash/繁简能力接进 `clean`,**不引入 JS、不在 schema 暴露 `java.*`**。

## Goals / Non-Goals

**Goals**
- 覆盖 Legado 反爬最常见组合:AES(CBC/ECB/CFB/GCM)+ key/iv 多种编码 + base64/hex 输入输出;base64/hex/url 编解码;md5/sha/hmac;t2s/s2t。
- schema 友好:每个算子是显式命名的结构化字段,可被 JSON-Schema/约束解码逐字段保护,AI 一次成型友好。
- 纯 Rust、零 C 依赖;crypto fn 内聚,供未来 JS `java.*` 后端复用。

**Non-Goals**
- RSA/非对称(v1 不做;后续随 JS `java.*` 后端或独立 change)。
- 任何控制流/条件/循环(那是 JS 引擎的职责)。
- Legado `java.*` 字符串语法兼容(转换器范畴)。

## Decisions

### D1 — 扩展 `CleanStep`,而非新增 `Rule::Transform`
评估三方案:
- (a) **扩展 `CleanStep` 新增 `Option` 算子字段** ✅ 选定。`clean` 已是天然有序流水线,"取串→base64 解码→AES 解密→trim"正是真实书源需求;crypto 是确定性 reduce,与 regex 同构,放此层最自然。
- (b) 富类型 `transform: [TransformOp]` 步列表 —— 与现有 `clean` 重复,徒增概念。
- (c) 新增 `Rule::Transform` 变体 —— 会重载 `Rule` 的"数据抽取"语义,污染 AST。排除。

`CleanStep` 带 `#[serde(deny_unknown_fields)]`:新字段必须显式声明 + `#[serde(default, skip_serializing_if = "Option::is_none")]`,老 JSON 不含新字段 → `None` → 行为不变(后向兼容)。

### D2 — 步内固定顺序
单个 `CleanStep` 内多算子按**固定顺序**执行,文档明示:
`regex/replace → trim → prepend → append → decode → encode → hash → cipher → cn`。
真实用法多为"一步一算子"(靠多个 step 串联),固定顺序仅为消除歧义。

### D3 — `apply_clean` 升级为 `Result<String, EvalError>`
解码(非法 base64)、解密(padding 错、key 错)会失败。`apply_clean` 改 `-> Result<String, EvalError>`,eval.rs:43 调用处加 `?`。新增 `EvalError::Crypto(String)` / `EvalError::Codec(String)`。这同时为 `js-rule-engine` 的 `eval_js` 错误传播铺好路。

### D4 — `CipherStep` 字段设计(覆盖 Legado key/iv 编码组合)
```jsonc
{
  "algo": "aes",            // aes | des | tripleDes
  "mode": "cbc",            // cbc | ecb | cfb | gcm
  "padding": "pkcs7",       // pkcs7 | zero | none   (默认 pkcs7;gcm 忽略)
  "op": "decrypt",          // decrypt | encrypt     (默认 decrypt;书源多为解密)
  "key": "…",               // 密钥材料(字符串)
  "keyEnc": "utf8",         // utf8 | base64 | hex   key 字符串→字节 (默认 utf8)
  "iv": "…",                // CBC/CFB/GCM 需要
  "ivEnc": "utf8",          // 同上 (默认 utf8)
  "inputEnc": "base64",     // 入参密文串→字节: base64 | hex | utf8 | raw (decrypt 默认 base64)
  "outputEnc": "utf8"       // 结果字节→串: utf8 | base64 | hex (decrypt 默认 utf8)
}
```
- decrypt:`input(str) --inputEnc--> bytes --decrypt--> bytes --outputEnc(默认 utf8)--> str`。
- encrypt:`input(str) --inputEnc(默认 utf8)--> bytes --encrypt--> bytes --outputEnc(默认 base64)--> str`。
- 默认值贴合"解密正文"主场景,使最简书源只需 `{ "cipher": { "key": "...", "iv": "..." } }`。

### D5 — crate 选型(钉死代际 cipher 0.4 / digest 0.10)
crates.io 上 RustCrypto 存在两套互不兼容 trait 代际。因 `aes-gcm 0.10` 依赖 `cipher ^0.4`、`hmac 0.12` 依赖 `digest ^0.10`,**全 crate 钉在代际 cipher 0.4 / digest 0.10**:

| 用途 | crate |
|---|---|
| AES ECB/CBC/CFB | `aes` `cbc` `cfb8` `ecb` `block-padding` `cipher` |
| AES-GCM | `aes-gcm`(密文=ct‖tag,与 Java 一致) |
| DES/3DES | `des`(复用同 cbc/ecb/padding) |
| base64 / hex / url | `base64` `hex` `percent-encoding` |
| md5 / sha / hmac | `md-5` `sha1` `sha2` `hmac` |
| 繁简 | **`ferrous-opencc`**(纯 Rust 重实现 OpenCC + 内置词典) |

### D6 — 繁简转换:`ferrous-opencc`,禁用 `opencc-rust`
`opencc-rust` 硬依赖系统 `libopencc`+`libstdc++`,只支持 Linux,会重演 onnxruntime 的 CRT/交叉编译之痛(CLAUDE.md `msvc-crt-static=false` 同源教训)。改用纯 Rust 的 `ferrous-opencc`(内置真实 OpenCC 词典、词组级精度,与 Legado 行为最接近)。
**实现期校验**:① 确认 crate 可用且 API 稳定,否则退 `fast2s`(仅 t2s,轻量)或 `character_converter`;② 测量词典对 cargo-dist 产物体积的影响——**若体积不可接受,把 `cn` 算子收进默认开启的 `cn-convert` feature**(schema 字段恒在,feature 关时运行期返回 `EvalError::Unsupported("cn-convert")`,镜像 xpath 模式)。

### D7 — 失败语义
- 解码/解密失败 → `Err(EvalError::Codec/Crypto)`,沿 `eval_value` 上抛。注意:`firstOf` 子规则求值出错会中断该分支——书源应保证 cipher 步配置正确;诊断由 `verify`/doctor 体检兜。
- 与"选择器无匹配返回空串"不同:crypto 失败是**配置/数据错误**,应显式报错而非静默空,便于书源作者定位。

## Risks / Trade-offs
- **二进制体积**:opencc 词典数 MB。缓解见 D6(必要时 feature gate)。
- **代际锁定**:升级任一 crypto crate 需整组同步,文档在 Cargo.toml 注释钉死代际原因。
- **firstOf 内 crypto 报错中断分支**:可接受;crypto 通常在确定的 content/url 规则上,不在回退链里。

## Migration Plan
纯新增字段。重新生成 `book-source.schema.json` 并更新 golden 测试。旧书源零影响。

## Open Questions
- `cn` 是否最终 feature-gate —— 取决于实现期实测体积(D6)。
