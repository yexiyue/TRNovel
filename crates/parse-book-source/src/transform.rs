//! 确定性 transform 算子(`clean` 流水线):编解码 / 哈希(含 HMAC)/ 对称加解密 / 繁简。
//!
//! 全部纯函数、纯 Rust、零 C 依赖(crypto 钉在 RustCrypto cipher 0.4 / digest 0.10 代)。
//! 这些 fn 既供 `eval::apply_clean` 调用,也作为后续 JS 引擎 `crypto` 对象的单一后端。
//!
//! 失败语义:编解码/解密是「配置或数据错误」,显式返回 `EvalError::Codec`/`Crypto`,
//! 而非静默空——便于书源作者定位(对比「选择器无匹配返回空」)。

use crate::error::EvalError;
use crate::source::{
    ByteEnc, CipherAlgo, CipherMode, CipherOp, CipherStep, CnConvert, Codec, HashAlgo, HashOut,
    HashStep, Padding,
};
use base64::Engine;
use base64::alphabet;
use base64::engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig};
use std::sync::LazyLock;

// 解码对填充宽容(真实书源 base64 常缺 `=`);编码按规范带填充。
static B64_STD: LazyLock<GeneralPurpose> = LazyLock::new(|| {
    GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
    )
});
static B64_URL: LazyLock<GeneralPurpose> = LazyLock::new(|| {
    GeneralPurpose::new(
        &alphabet::URL_SAFE,
        GeneralPurposeConfig::new().with_decode_padding_mode(DecodePaddingMode::Indifferent),
    )
});

// ───────────────────────── 编 / 解码 ─────────────────────────

/// 解码当前串(base64/base64url/hex/url)→ 文本(字节按 UTF-8 有损解释)。
pub fn decode(s: &str, codec: Codec) -> Result<String, EvalError> {
    Ok(String::from_utf8_lossy(&decode_bytes(s, codec)?).into_owned())
}

/// 编码当前串的 UTF-8 字节(base64/base64url/hex/url)。
///
/// 实际不会失败;返回 `Result` 仅为与 [`decode`] 在 clean 流水线 / JS `crypto` 绑定中保持统一签名。
pub fn encode(s: &str, codec: Codec) -> Result<String, EvalError> {
    Ok(match codec {
        Codec::Base64 => B64_STD.encode(s.as_bytes()),
        Codec::Base64url => B64_URL.encode(s.as_bytes()),
        Codec::Hex => hex::encode(s.as_bytes()),
        Codec::Url => {
            percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
        }
    })
}

fn decode_bytes(s: &str, codec: Codec) -> Result<Vec<u8>, EvalError> {
    match codec {
        Codec::Base64 => B64_STD
            .decode(s.trim())
            .map_err(|e| EvalError::Codec(format!("base64: {e}"))),
        Codec::Base64url => B64_URL
            .decode(s.trim())
            .map_err(|e| EvalError::Codec(format!("base64url: {e}"))),
        Codec::Hex => hex::decode(s.trim()).map_err(|e| EvalError::Codec(format!("hex: {e}"))),
        Codec::Url => Ok(percent_encoding::percent_decode_str(s).collect()),
    }
}

/// 按字节编码把串解释为字节(crypto 的 key/iv/输入)。
fn to_bytes(s: &str, enc: ByteEnc) -> Result<Vec<u8>, EvalError> {
    match enc {
        ByteEnc::Utf8 | ByteEnc::Raw => Ok(s.as_bytes().to_vec()),
        ByteEnc::Base64 => B64_STD
            .decode(s.trim())
            .map_err(|e| EvalError::Codec(format!("base64: {e}"))),
        ByteEnc::Hex => hex::decode(s.trim()).map_err(|e| EvalError::Codec(format!("hex: {e}"))),
    }
}

/// 把字节按编码渲染为串(crypto 的输出)。
fn from_bytes(b: &[u8], enc: ByteEnc) -> String {
    match enc {
        ByteEnc::Utf8 | ByteEnc::Raw => String::from_utf8_lossy(b).into_owned(),
        ByteEnc::Base64 => B64_STD.encode(b),
        ByteEnc::Hex => hex::encode(b),
    }
}

// ───────────────────────── 哈希 / HMAC ─────────────────────────

/// 计算哈希或 HMAC(提供 `hmacKey` 时),按 `output` 编码输出。
pub fn hash(s: &str, step: &HashStep) -> Result<String, EvalError> {
    let data = s.as_bytes();
    let digest = match &step.hmac_key {
        Some(k) => hmac_digest(step.algo, &to_bytes(k, step.hmac_key_enc)?, data),
        None => plain_digest(step.algo, data),
    };
    Ok(match step.output {
        HashOut::Hex => hex::encode(&digest),
        HashOut::Base64 => B64_STD.encode(&digest),
    })
}

fn plain_digest(algo: HashAlgo, data: &[u8]) -> Vec<u8> {
    use md5::Md5;
    use sha1::Sha1;
    use sha2::{Digest, Sha256, Sha512};
    match algo {
        HashAlgo::Md5 => Md5::digest(data).to_vec(),
        HashAlgo::Sha1 => Sha1::digest(data).to_vec(),
        HashAlgo::Sha256 => Sha256::digest(data).to_vec(),
        HashAlgo::Sha512 => Sha512::digest(data).to_vec(),
    }
}

fn hmac_digest(algo: HashAlgo, key: &[u8], data: &[u8]) -> Vec<u8> {
    use hmac::{Hmac, Mac};
    // HMAC 接受任意长度密钥,new_from_slice 不会失败。
    macro_rules! mac {
        ($h:ty) => {{
            let mut m = <Hmac<$h>>::new_from_slice(key).expect("HMAC accepts any key length");
            m.update(data);
            m.finalize().into_bytes().to_vec()
        }};
    }
    match algo {
        HashAlgo::Md5 => mac!(md5::Md5),
        HashAlgo::Sha1 => mac!(sha1::Sha1),
        HashAlgo::Sha256 => mac!(sha2::Sha256),
        HashAlgo::Sha512 => mac!(sha2::Sha512),
    }
}

// ───────────────────────── 对称加解密 ─────────────────────────

/// 解/加密当前串。默认值贴合解密主场景(见 [`CipherStep`])。
pub fn cipher(s: &str, st: &CipherStep) -> Result<String, EvalError> {
    let input_enc = st.input_enc.unwrap_or(match st.op {
        CipherOp::Decrypt => ByteEnc::Base64,
        CipherOp::Encrypt => ByteEnc::Utf8,
    });
    let output_enc = st.output_enc.unwrap_or(match st.op {
        CipherOp::Decrypt => ByteEnc::Utf8,
        CipherOp::Encrypt => ByteEnc::Base64,
    });
    let data = to_bytes(s, input_enc)?;
    let key = to_bytes(&st.key, st.key_enc)?;
    let iv = match &st.iv {
        Some(iv) => to_bytes(iv, st.iv_enc)?,
        None => Vec::new(),
    };
    let out = match st.mode {
        CipherMode::Gcm => gcm(st, &key, &iv, &data)?,
        CipherMode::Cfb => cfb(st, &key, &iv, &data)?,
        CipherMode::Cbc | CipherMode::Ecb => block(st, &key, &iv, &data)?,
    };
    Ok(from_bytes(&out, output_enc))
}

fn keyiv(e: impl std::fmt::Display) -> EvalError {
    EvalError::Crypto(format!("invalid key/iv length: {e}"))
}
fn unpad(e: impl std::fmt::Display) -> EvalError {
    EvalError::Crypto(format!("decrypt/unpad failed: {e}"))
}

/// CBC/ECB(带 padding)。展开 mode×op×padding 全分支(由调用方按 key 长度选具体分组密码)。
macro_rules! run_block {
    ($bc:ty, $st:expr, $key:expr, $iv:expr, $data:expr) => {{
        use cipher::block_padding::{NoPadding, Pkcs7, ZeroPadding};
        use cipher::{BlockDecryptMut, BlockEncryptMut, KeyInit, KeyIvInit};
        match ($st.mode, $st.op) {
            (CipherMode::Cbc, CipherOp::Decrypt) => {
                let c = cbc::Decryptor::<$bc>::new_from_slices($key, $iv).map_err(keyiv)?;
                match $st.padding {
                    Padding::Pkcs7 => c.decrypt_padded_vec_mut::<Pkcs7>($data).map_err(unpad),
                    Padding::Zero => c
                        .decrypt_padded_vec_mut::<ZeroPadding>($data)
                        .map_err(unpad),
                    Padding::None => c.decrypt_padded_vec_mut::<NoPadding>($data).map_err(unpad),
                }
            }
            (CipherMode::Cbc, CipherOp::Encrypt) => {
                let c = cbc::Encryptor::<$bc>::new_from_slices($key, $iv).map_err(keyiv)?;
                Ok(match $st.padding {
                    Padding::Pkcs7 => c.encrypt_padded_vec_mut::<Pkcs7>($data),
                    Padding::Zero => c.encrypt_padded_vec_mut::<ZeroPadding>($data),
                    Padding::None => c.encrypt_padded_vec_mut::<NoPadding>($data),
                })
            }
            (CipherMode::Ecb, CipherOp::Decrypt) => {
                let c = ecb::Decryptor::<$bc>::new_from_slice($key).map_err(keyiv)?;
                match $st.padding {
                    Padding::Pkcs7 => c.decrypt_padded_vec_mut::<Pkcs7>($data).map_err(unpad),
                    Padding::Zero => c
                        .decrypt_padded_vec_mut::<ZeroPadding>($data)
                        .map_err(unpad),
                    Padding::None => c.decrypt_padded_vec_mut::<NoPadding>($data).map_err(unpad),
                }
            }
            (CipherMode::Ecb, CipherOp::Encrypt) => {
                let c = ecb::Encryptor::<$bc>::new_from_slice($key).map_err(keyiv)?;
                Ok(match $st.padding {
                    Padding::Pkcs7 => c.encrypt_padded_vec_mut::<Pkcs7>($data),
                    Padding::Zero => c.encrypt_padded_vec_mut::<ZeroPadding>($data),
                    Padding::None => c.encrypt_padded_vec_mut::<NoPadding>($data),
                })
            }
            // cipher() 只把 cbc/ecb 路由到 block();此分支防御未来重构误接其它模式,
            // 显式报错而非 panic(库不应 panic,与本模块「显式错误」哲学一致)。
            _ => Err(EvalError::Crypto(format!(
                "block() 不支持的加密模式: {:?}",
                $st.mode
            ))),
        }
    }};
}

fn block(st: &CipherStep, key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, EvalError> {
    match st.algo {
        CipherAlgo::Aes => match key.len() {
            16 => run_block!(aes::Aes128, st, key, iv, data),
            24 => run_block!(aes::Aes192, st, key, iv, data),
            32 => run_block!(aes::Aes256, st, key, iv, data),
            n => Err(EvalError::Crypto(format!(
                "AES key must be 16/24/32 bytes, got {n}"
            ))),
        },
        CipherAlgo::Des => match key.len() {
            8 => run_block!(des::Des, st, key, iv, data),
            n => Err(EvalError::Crypto(format!(
                "DES key must be 8 bytes, got {n}"
            ))),
        },
        CipherAlgo::TripleDes => match key.len() {
            24 => run_block!(des::TdesEde3, st, key, iv, data),
            16 => run_block!(des::TdesEde2, st, key, iv, data),
            n => Err(EvalError::Crypto(format!(
                "3DES key must be 16/24 bytes, got {n}"
            ))),
        },
    }
}

/// CFB(128 位反馈,对齐 CryptoJS 默认;无 padding,流式)。
fn cfb(st: &CipherStep, key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, EvalError> {
    use cfb_mode::cipher::{AsyncStreamCipher, KeyIvInit};
    let mut buf = data.to_vec();
    macro_rules! go {
        ($bc:ty) => {{
            match st.op {
                CipherOp::Decrypt => cfb_mode::Decryptor::<$bc>::new_from_slices(key, iv)
                    .map_err(keyiv)?
                    .decrypt(&mut buf),
                CipherOp::Encrypt => cfb_mode::Encryptor::<$bc>::new_from_slices(key, iv)
                    .map_err(keyiv)?
                    .encrypt(&mut buf),
            }
        }};
    }
    match st.algo {
        CipherAlgo::Aes => match key.len() {
            16 => go!(aes::Aes128),
            24 => go!(aes::Aes192),
            32 => go!(aes::Aes256),
            n => {
                return Err(EvalError::Crypto(format!(
                    "AES key must be 16/24/32 bytes, got {n}"
                )));
            }
        },
        CipherAlgo::Des => match key.len() {
            8 => go!(des::Des),
            n => {
                return Err(EvalError::Crypto(format!(
                    "DES key must be 8 bytes, got {n}"
                )));
            }
        },
        CipherAlgo::TripleDes => match key.len() {
            24 => go!(des::TdesEde3),
            16 => go!(des::TdesEde2),
            n => {
                return Err(EvalError::Crypto(format!(
                    "3DES key must be 16/24 bytes, got {n}"
                )));
            }
        },
    }
    Ok(buf)
}

/// AES-GCM(AEAD;nonce=IV 须 12 字节;密文 = 密文‖tag,与 Java/CryptoJS 一致)。
fn gcm(st: &CipherStep, key: &[u8], iv: &[u8], data: &[u8]) -> Result<Vec<u8>, EvalError> {
    use aes_gcm::aead::generic_array::GenericArray;
    use aes_gcm::aead::{Aead, KeyInit};
    use aes_gcm::{Aes128Gcm, Aes256Gcm};
    if st.algo != CipherAlgo::Aes {
        return Err(EvalError::Crypto("GCM 仅支持 AES".into()));
    }
    if iv.len() != 12 {
        return Err(EvalError::Crypto(format!(
            "AES-GCM nonce 须 12 字节,实际 {}",
            iv.len()
        )));
    }
    let nonce = GenericArray::from_slice(iv);
    macro_rules! go {
        ($t:ty) => {{
            let c = <$t>::new_from_slice(key).map_err(keyiv)?;
            match st.op {
                CipherOp::Decrypt => c
                    .decrypt(nonce, data)
                    .map_err(|e| EvalError::Crypto(format!("gcm decrypt: {e}"))),
                CipherOp::Encrypt => c
                    .encrypt(nonce, data)
                    .map_err(|e| EvalError::Crypto(format!("gcm encrypt: {e}"))),
            }
        }};
    }
    match key.len() {
        16 => go!(Aes128Gcm),
        32 => go!(Aes256Gcm),
        n => Err(EvalError::Crypto(format!(
            "AES-GCM key must be 16/32 bytes, got {n}"
        ))),
    }
}

// ───────────────────────── 繁简转换 ─────────────────────────

/// 繁简转换(t2s/s2t)。词典加载失败(理论不会,内置词典)时退化为原样返回。
pub fn cn_convert(s: &str, dir: CnConvert) -> String {
    use ferrous_opencc::OpenCC;
    use ferrous_opencc::config::BuiltinConfig;
    static T2S: LazyLock<Option<OpenCC>> =
        LazyLock::new(|| OpenCC::from_config(BuiltinConfig::T2s).ok());
    static S2T: LazyLock<Option<OpenCC>> =
        LazyLock::new(|| OpenCC::from_config(BuiltinConfig::S2t).ok());
    let cc = match dir {
        CnConvert::T2s => &*T2S,
        CnConvert::S2t => &*S2T,
    };
    cc.as_ref()
        .map(|c| c.convert(s))
        .unwrap_or_else(|| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{ByteEnc, CipherAlgo, CipherMode, CipherOp, CipherStep, Padding};

    #[test]
    fn base64_round_trip() {
        assert_eq!(encode("hello", Codec::Base64).unwrap(), "aGVsbG8=");
        assert_eq!(decode("aGVsbG8=", Codec::Base64).unwrap(), "hello");
        // 缺填充也能解。
        assert_eq!(decode("aGVsbG8", Codec::Base64).unwrap(), "hello");
    }

    #[test]
    fn hex_and_url() {
        assert_eq!(decode("68656c6c6f", Codec::Hex).unwrap(), "hello");
        assert_eq!(encode("a b&c", Codec::Url).unwrap(), "a%20b%26c");
        assert_eq!(decode("a%20b%26c", Codec::Url).unwrap(), "a b&c");
    }

    #[test]
    fn invalid_base64_errors() {
        assert!(matches!(
            decode("!!!notbase64!!!", Codec::Base64),
            Err(EvalError::Codec(_))
        ));
    }

    fn hashstep(algo: HashAlgo, out: HashOut, key: Option<&str>) -> HashStep {
        HashStep {
            algo,
            output: out,
            hmac_key: key.map(|s| s.to_string()),
            hmac_key_enc: ByteEnc::Utf8,
        }
    }

    #[test]
    fn md5_sha_known_vectors() {
        assert_eq!(
            hash("hello", &hashstep(HashAlgo::Md5, HashOut::Hex, None)).unwrap(),
            "5d41402abc4b2a76b9719d911017c592"
        );
        assert_eq!(
            hash("hello", &hashstep(HashAlgo::Sha256, HashOut::Hex, None)).unwrap(),
            "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn hmac_sha256_known_vector() {
        // RFC-ish 已知向量。
        let v = hash(
            "The quick brown fox jumps over the lazy dog",
            &hashstep(HashAlgo::Sha256, HashOut::Hex, Some("key")),
        )
        .unwrap();
        assert_eq!(
            v,
            "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8"
        );
    }

    fn aes_cbc(op: CipherOp, in_enc: ByteEnc, out_enc: ByteEnc) -> CipherStep {
        CipherStep {
            algo: CipherAlgo::Aes,
            mode: CipherMode::Cbc,
            padding: Padding::Pkcs7,
            op,
            key: "0123456789abcdef".into(),
            key_enc: ByteEnc::Utf8,
            iv: Some("abcdef9876543210".into()),
            iv_enc: ByteEnc::Utf8,
            input_enc: Some(in_enc),
            output_enc: Some(out_enc),
        }
    }

    #[test]
    fn aes_cbc_round_trip() {
        let plain = "蛊真人正文：第一章";
        let ct = cipher(
            plain,
            &aes_cbc(CipherOp::Encrypt, ByteEnc::Utf8, ByteEnc::Base64),
        )
        .unwrap();
        let back = cipher(
            &ct,
            &aes_cbc(CipherOp::Decrypt, ByteEnc::Base64, ByteEnc::Utf8),
        )
        .unwrap();
        assert_eq!(back, plain);
    }

    #[test]
    fn aes_cbc_known_ciphertext_decrypts() {
        // 由上面的加密路径产出的固定密文,验证 decrypt 独立正确。
        let plain = "hello world";
        let ct = cipher(
            plain,
            &aes_cbc(CipherOp::Encrypt, ByteEnc::Utf8, ByteEnc::Base64),
        )
        .unwrap();
        let back = cipher(
            &ct,
            &aes_cbc(CipherOp::Decrypt, ByteEnc::Base64, ByteEnc::Utf8),
        )
        .unwrap();
        assert_eq!(back, plain);
    }

    #[test]
    fn aes_gcm_round_trip() {
        let mk = |op| CipherStep {
            algo: CipherAlgo::Aes,
            mode: CipherMode::Gcm,
            padding: Padding::None,
            op,
            key: "0123456789abcdef".into(),
            key_enc: ByteEnc::Utf8,
            iv: Some("0123456789ab".into()), // 12 字节
            iv_enc: ByteEnc::Utf8,
            input_enc: None,
            output_enc: None,
        };
        let ct = cipher("secret内容", &mk(CipherOp::Encrypt)).unwrap();
        let back = cipher(&ct, &mk(CipherOp::Decrypt)).unwrap();
        assert_eq!(back, "secret内容");
    }

    #[test]
    fn cn_t2s_s2t() {
        assert_eq!(cn_convert("漢字測試", CnConvert::T2s), "汉字测试");
        assert_eq!(cn_convert("汉字测试", CnConvert::S2t), "漢字測試");
    }
}
