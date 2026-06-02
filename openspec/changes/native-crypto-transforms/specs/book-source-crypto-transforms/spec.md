## ADDED Requirements

### Requirement: clean 流水线的确定性 transform 算子

`clean` 流水线 SHALL 支持确定性加解密/编码/哈希/繁简算子。`CleanStep` MUST 新增 `decode`、`encode`、`hash`、`cipher`、`cn` 字段(均可选),实现 MUST 为纯 Rust、零 C 依赖。单个 `CleanStep` 内多算子 MUST 按固定顺序执行:`regex → trim → prepend → append → decode → encode → hash → cipher → cn`。

#### Scenario: Base64 解码

- **WHEN** 一个 clean 步为 `{ "decode": "base64" }`,输入为 Base64 字符串
- **THEN** 返回解码后的明文字符串

#### Scenario: AES-CBC 解密正文

- **WHEN** clean 链为 `[{ "decode": "base64" }, { "cipher": { "algo": "aes", "mode": "cbc", "key": "...", "iv": "..." } }]`,输入为 Base64 密文
- **THEN** 先 Base64 解码为字节,再以 AES-CBC + PKCS7 解密,返回 UTF-8 明文

#### Scenario: MD5 签名

- **WHEN** clean 步为 `{ "hash": { "algo": "md5", "output": "hex" } }`
- **THEN** 返回输入的 MD5 十六进制摘要

#### Scenario: 繁简转换

- **WHEN** clean 步为 `{ "cn": "t2s" }`,输入为繁体中文
- **THEN** 返回对应简体中文

### Requirement: clean 算子覆盖常见编码与密钥组合

`cipher` 算子 SHALL 支持 AES(CBC/ECB/CFB/GCM)、DES、3DES,padding 为 pkcs7/zero/none;密钥与 IV SHALL 支持 utf8/base64/hex 编码;输入密文与输出 SHALL 支持 base64/hex/utf8 编码;`op` SHALL 支持 decrypt 与 encrypt。`decode`/`encode` SHALL 支持 base64、base64url、hex、url(百分号)。`hash` SHALL 支持 md5/sha1/sha256/sha512,输出 hex 或 base64,并 SHALL 支持 HMAC(提供 `hmacKey` 时)。

#### Scenario: 密钥以 base64 编码提供

- **WHEN** `cipher` 配置 `keyEnc: "base64"`,`key` 为 Base64 字符串
- **THEN** 密钥先按 Base64 解码为字节再用于加解密

#### Scenario: HMAC-SHA256

- **WHEN** `hash` 配置 `{ "algo": "sha256", "hmacKey": "secret", "output": "hex" }`
- **THEN** 返回以 secret 为密钥的 HMAC-SHA256 十六进制结果

### Requirement: clean 失败显式报错且后向兼容

当 `decode`/`cipher` 等算子因非法输入或错误密钥失败时,`apply_clean` MUST 返回求值错误(`EvalError::Codec` / `EvalError::Crypto`)而非静默空结果;`apply_clean` MUST 升级为可失败签名,其错误 MUST 沿规则求值上抛。不含新算子字段的既有书源 MUST 反序列化为默认空算子,行为与改造前完全一致。

#### Scenario: 非法 Base64 输入报错

- **WHEN** `{ "decode": "base64" }` 的输入不是合法 Base64
- **THEN** 返回 `EvalError::Codec` 错误,而非空串

#### Scenario: 旧书源无新算子字段向后兼容

- **WHEN** 一个不含 decode/encode/hash/cipher/cn 字段的既有书源被解析与求值
- **THEN** clean 行为与改造前一致(仅 regex/trim/prepend/append 生效)
