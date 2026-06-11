//! `clean` 后处理流水线的算子配置类型。
//!
//! 一个 [`CleanStep`] 聚合若干算子,步内按固定顺序执行:
//! `regex/replace → trim → prepend → append → decode → encode → hash → cipher → fontMap → cn → js`。
//! 这里只是**配置**(纯 serde 数据);确定性实现在 `crate::eval::transform`。

use serde::{Deserialize, Serialize};

/// 编解码方式(`decode`/`encode` 算子,以及 crypto 的字节↔串编码)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Codec {
    Base64,
    Base64url,
    Hex,
    /// URL 百分号编解码。
    Url,
}

/// crypto 的 key/iv/输入/输出字节编码。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum ByteEnc {
    #[default]
    Utf8,
    Base64,
    Hex,
    /// 原样字节(等同 utf8 字节,主要用于输入密文已是裸字节串的场景)。
    Raw,
}

/// 哈希算法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum HashAlgo {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

/// 哈希/HMAC 输出编码。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum HashOut {
    #[default]
    Hex,
    Base64,
}

/// 哈希算子(可选 HMAC)。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct HashStep {
    pub algo: HashAlgo,
    #[serde(default)]
    pub output: HashOut,
    /// 提供则计算 HMAC(以此为密钥)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hmac_key: Option<String>,
    #[serde(default)]
    pub hmac_key_enc: ByteEnc,
}

/// 对称加密算法。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum CipherAlgo {
    Aes,
    Des,
    TripleDes,
}

/// 加密模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum CipherMode {
    Cbc,
    Ecb,
    Cfb,
    Gcm,
}

/// 填充方式(gcm 忽略)。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum Padding {
    #[default]
    Pkcs7,
    Zero,
    None,
}

/// 加解密方向。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum CipherOp {
    #[default]
    Decrypt,
    Encrypt,
}

/// 加解密算子。默认值贴合「解密正文」主场景:`op=decrypt`、`inputEnc=base64`、`outputEnc=utf8`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CipherStep {
    pub algo: CipherAlgo,
    pub mode: CipherMode,
    #[serde(default)]
    pub padding: Padding,
    #[serde(default)]
    pub op: CipherOp,
    pub key: String,
    #[serde(default)]
    pub key_enc: ByteEnc,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub iv: Option<String>,
    #[serde(default)]
    pub iv_enc: ByteEnc,
    /// 入参密文串→字节;省略时按 `op` 取默认(decrypt→base64,encrypt→utf8)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub input_enc: Option<ByteEnc>,
    /// 结果字节→串;省略时按 `op` 取默认(decrypt→utf8,encrypt→base64)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output_enc: Option<ByteEnc>,
}

/// 繁简转换方向。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub enum CnConvert {
    /// 繁体 → 简体。
    T2s,
    /// 简体 → 繁体。
    S2t,
}

/// 单步后处理。步内多算子按固定顺序执行:
/// `regex/replace → trim → prepend → append → decode → encode → hash → cipher → fontMap → cn`。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct CleanStep {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regex: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prepend: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub append: Option<String>,
    /// 解码(base64/base64url/hex/url)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decode: Option<Codec>,
    /// 编码(base64/base64url/hex/url)。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub encode: Option<Codec>,
    /// 哈希/HMAC。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hash: Option<HashStep>,
    /// 对称加解密。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cipher: Option<CipherStep>,
    /// 字体反爬还原:私有区(PUA)字符按映射表换回真字。键为码点十六进制(如 `"E4DE"` 或 `"U+E4DE"`),
    /// 值为目标字符;表外字符原样保留。用于番茄等「自定义字体 + PUA」反爬站点——表是数据,由书源内联
    /// 提供(引擎不内置任何站点的表),可用 `trn gen-fontmap` 生成。
    #[serde(default, rename = "fontMap", skip_serializing_if = "Option::is_none")]
    pub font_map: Option<std::collections::BTreeMap<String, String>>,
    /// 繁简转换。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cn: Option<CnConvert>,
    /// JS 后处理(逃生舱;脚本里以当前串为 `result`)。需启用 `js` feature。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub js: Option<String>,
}
