//! per-source 持久状态(`js-host` feature):跨请求 KV、书源级用户变量、登录态 header、
//! 加密凭据、按域名归并的 cookie。是登录与多步请求编排的状态层(见 change `js-host-bridge`)。
//!
//! 命名空间:每个书源按其 url 的 md5 各存一份,文件路径由调用方(app)给定——
//! `parse-book-source` 不硬编码 `~/.novel` 路径,保持库的纯净。
//! 凭据(`login_info`)用**机器绑定**的 AES key(machine-uid 派生)加密,与明文 `login_header` 分桶。

use crate::error::EvalError;
use crate::eval::transform;
use crate::source::{
    ByteEnc, CipherAlgo, CipherMode, CipherOp, CipherStep, HashAlgo, HashOut, HashStep, Padding,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// 一个书源的持久状态。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SourceState {
    /// 跨请求 KV(对应 `source.put/get` 的便签)。
    pub kv: BTreeMap<String, String>,
    /// 书源级用户变量(UI 可编辑的单一配置槽)。
    pub variable: String,
    /// 登录态请求头(明文):JWT(`Authorization: Bearer`)/ 自定义 token 头 / Cookie 均可。
    /// 每个**同注册域**请求自动 merge 进 header(JWT 与 Cookie 走同一条注入路径;
    /// 跨注册域请求跳过,防凭据外泄,见 `cookie::merge_login_into_headers`)。
    pub login_header: BTreeMap<String, String>,
    /// 登录凭据(账号密码等)的 AES 密文;机器绑定密钥,拷到别的设备不可解。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub login_info: Option<String>,
    /// 按注册域归并的持久 cookie(`domain -> "k=v; k2=v2"`)。
    pub cookies: BTreeMap<String, String>,
    /// 登录态过期时刻(unix 秒);`None` = 永不过期。读时校验,过期则 [`Self::clear_login`]。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expire_at: Option<u64>,
}

impl SourceState {
    /// 从文件加载;不存在或损坏则返回默认(空)状态。
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    /// 保存到文件(自动建父目录)。
    ///
    /// 文件含 `login_header`(明文 token/Cookie)与 cookie,故在 unix 上以 **0600** 落盘
    /// (仅属主可读写),与 `login_info` 的 AES 加密配套——多用户机上避免凭据被同组/他人读取。
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_string_pretty(self).unwrap_or_default();
        #[cfg(unix)]
        {
            use std::io::Write;
            use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
            // 新建文件即以 0600 创建(避免短暂 0644 窗口);已存在文件再显式收紧。
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .mode(0o600)
                .open(path)?;
            f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
            f.write_all(json.as_bytes())?;
            Ok(())
        }
        #[cfg(not(unix))]
        {
            std::fs::write(path, json)
        }
    }

    /// 加密写入登录凭据(机器绑定密钥)。
    ///
    /// 每次生成**随机 16 字节 IV**(CBC 固定 IV 是确定性加密:相同凭据密文相同、公共前缀
    /// 首块可比对),存储格式为 `base64(iv):base64(密文)`;旧格式(无 `:`、机器派生固定 IV)
    /// 由 [`Self::get_login_info`] 兼容读取。
    pub fn set_login_info(&mut self, plain: &str) -> Result<(), EvalError> {
        let key = machine_key()?;
        let iv = random_iv();
        let ct = cipher_with(plain, &key, &hex::encode(iv), CipherOp::Encrypt)?;
        use base64::Engine as _;
        let iv_b64 = base64::engine::general_purpose::STANDARD.encode(iv);
        self.login_info = Some(format!("{iv_b64}:{ct}"));
        Ok(())
    }

    /// 解密读取登录凭据;未设置返回 `None`。
    /// 新格式 `base64(iv):base64(密文)`;无 `:` 分隔符按旧格式(固定 IV)回退解密
    /// (解密失败如 padding 错误,经 transform 层映射为 [`EvalError::Crypto`],不 panic)。
    pub fn get_login_info(&self) -> Result<Option<String>, EvalError> {
        let Some(stored) = &self.login_info else {
            return Ok(None);
        };
        let key = machine_key()?;
        let (iv_hex, ct) = match stored.split_once(':') {
            Some((iv_b64, ct)) => {
                use base64::Engine as _;
                let iv = base64::engine::general_purpose::STANDARD
                    .decode(iv_b64)
                    .map_err(|e| EvalError::Crypto(format!("login_info IV 解码失败: {e}")))?;
                (hex::encode(iv), ct)
            }
            None => (legacy_machine_iv()?, stored.as_str()),
        };
        Ok(Some(cipher_with(ct, &key, &iv_hex, CipherOp::Decrypt)?))
    }

    /// 登录态是否已过期(`expire_at` 早于当前时刻)。无 `expire_at` 视为永不过期。
    pub fn is_login_expired(&self) -> bool {
        self.expire_at.is_some_and(|exp| now_secs() >= exp)
    }

    /// 清除登录态(loginHeader / loginInfo / cookies / expire_at),保留 kv 与 variable。
    /// 供过期或用户登出时调用。
    pub fn clear_login(&mut self) {
        self.login_header.clear();
        self.login_info = None;
        self.cookies.clear();
        self.expire_at = None;
    }

    /// 读时 TTL 校验:若已过期则清除登录态并返回 `true`(调用方据此落盘 / 提示重登)。
    pub fn purge_if_expired(&mut self) -> bool {
        if self.is_login_expired() {
            self.clear_login();
            true
        } else {
            false
        }
    }
}

/// 当前 unix 时间(秒);系统时钟早于纪元时返回 0。
fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// 由机器唯一标识派生 AES-256 key(hex)。
///
/// 防御:受限/容器/裁剪环境下 `machine-uid` 可能读到空串或平凡值(如 `/etc/machine-id`
/// 未初始化、macOS 早期启动返回全零),那样会派生出 `SHA256("")` 这类**公开可计算**
/// 的固定密钥,凭据「加密」形同明文。故此处校验 id 非空且长度足够,否则拒绝(报错优于伪加密)。
fn machine_key() -> Result<String, EvalError> {
    let id = machine_uid::get().map_err(|e| EvalError::Crypto(format!("machine-uid: {e}")))?;
    derive_key(checked_machine_id(&id)?)
}

/// 旧落盘格式的固定 IV(`MD5(machine_id)` hex)——仅供 [`SourceState::get_login_info`]
/// 解密历史数据,新写入一律随机 IV。
fn legacy_machine_iv() -> Result<String, EvalError> {
    let id = machine_uid::get().map_err(|e| EvalError::Crypto(format!("machine-uid: {e}")))?;
    derive_legacy_iv(checked_machine_id(&id)?)
}

/// 随机 16 字节 CBC IV(OsRng;经 aes-gcm 的 aead re-export 取得,免新增依赖)。
fn random_iv() -> [u8; 16] {
    use aes_gcm::aead::rand_core::RngCore;
    let mut iv = [0u8; 16];
    aes_gcm::aead::OsRng.fill_bytes(&mut iv);
    iv
}

/// 校验机器标识非空且非平凡(去掉 0/-/: 后仍有足够熵),否则拒绝——避免派生公开可计算的固定密钥。
fn checked_machine_id(id: &str) -> Result<&str, EvalError> {
    let trimmed = id.trim();
    if trimmed.trim_matches(['0', '-', ':']).len() < 8 {
        return Err(EvalError::Crypto(
            "machine-uid 为空/平凡值,拒绝以公开可计算的密钥加密凭据".into(),
        ));
    }
    Ok(trimmed)
}

/// 从任意标识派生 key(hex);抽出以便单测用固定标识验证加解密。
fn derive_key(id: &str) -> Result<String, EvalError> {
    transform::hash(id, &hashstep(HashAlgo::Sha256)) // 64 hex = 32 bytes(AES-256)
}

/// 从任意标识派生旧格式固定 IV(hex);仅用于解密历史落盘数据。
fn derive_legacy_iv(id: &str) -> Result<String, EvalError> {
    transform::hash(id, &hashstep(HashAlgo::Md5)) // 32 hex = 16 bytes(CBC IV)
}

fn hashstep(algo: HashAlgo) -> HashStep {
    HashStep {
        algo,
        output: HashOut::Hex,
        hmac_key: None,
        hmac_key_enc: ByteEnc::Utf8,
    }
}

/// AES-256-CBC 加/解密(key/iv 为 hex):加密 utf8→base64,解密 base64→utf8。
fn cipher_with(s: &str, key_hex: &str, iv_hex: &str, op: CipherOp) -> Result<String, EvalError> {
    transform::cipher(
        s,
        &CipherStep {
            algo: CipherAlgo::Aes,
            mode: CipherMode::Cbc,
            padding: Padding::Pkcs7,
            op,
            key: key_hex.to_string(),
            key_enc: ByteEnc::Hex,
            iv: Some(iv_hex.to_string()),
            iv_enc: ByteEnc::Hex,
            input_enc: None,  // 默认:encrypt=utf8 / decrypt=base64
            output_enc: None, // 默认:encrypt=base64 / decrypt=utf8
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_json_round_trip() {
        let mut s = SourceState::default();
        s.kv.insert("token".into(), "abc".into());
        s.login_header
            .insert("Authorization".into(), "Bearer xyz".into());
        s.cookies.insert("site.com".into(), "sid=1".into());
        s.variable = "user-cfg".into();
        let json = serde_json::to_string(&s).unwrap();
        let back: SourceState = serde_json::from_str(&json).unwrap();
        assert_eq!(s, back);
    }

    #[test]
    fn login_info_encrypt_round_trip() {
        // 用固定标识派生密钥,验证加解密往返(不依赖真实 machine-uid,CI 稳定)。
        let key = derive_key("fixed-machine-id").unwrap();
        let iv = derive_legacy_iv("fixed-machine-id").unwrap();
        let plain = r#"{"user":"alice","pass":"secret密码"}"#;
        let ct = cipher_with(plain, &key, &iv, CipherOp::Encrypt).unwrap();
        assert_ne!(ct, plain, "应已加密");
        let back = cipher_with(&ct, &key, &iv, CipherOp::Decrypt).unwrap();
        assert_eq!(back, plain);
    }

    // ── 审查/security:随机 IV——同明文两次加密密文不同(非确定性),且新格式往返可解 ──
    #[test]
    fn login_info_random_iv_nondeterministic() {
        let mut s1 = SourceState::default();
        let mut s2 = SourceState::default();
        s1.set_login_info("same-plain").unwrap();
        s2.set_login_info("same-plain").unwrap();
        assert_ne!(
            s1.login_info, s2.login_info,
            "随机 IV 下同明文两次加密的密文应不同"
        );
        assert!(
            s1.login_info.as_deref().unwrap().contains(':'),
            "新格式应为 base64(iv):base64(密文)"
        );
        assert_eq!(s1.get_login_info().unwrap().as_deref(), Some("same-plain"));
    }

    // ── 审查/compat:旧落盘格式(固定 IV、无分隔符)仍可解密 ──
    #[test]
    fn login_info_legacy_format_still_decrypts() {
        let key = machine_key().unwrap();
        let iv = legacy_machine_iv().unwrap();
        let ct = cipher_with("old-secret", &key, &iv, CipherOp::Encrypt).unwrap();
        assert!(!ct.contains(':'), "旧格式(base64 密文)不含分隔符");
        let s = SourceState {
            login_info: Some(ct),
            ..Default::default()
        };
        assert_eq!(s.get_login_info().unwrap().as_deref(), Some("old-secret"));
    }

    #[test]
    fn checked_machine_id_rejects_empty_and_trivial() {
        // 空 / 纯空白 / 全零 / 短 id 一律拒绝(否则会派生公开可计算的固定密钥)。
        assert!(checked_machine_id("").is_err());
        assert!(checked_machine_id("   ").is_err());
        assert!(checked_machine_id("00000000-0000-0000").is_err());
        assert!(checked_machine_id("abc").is_err());
        // 正常 machine-uid(uuid 形态)通过,并 trim。
        assert_eq!(
            checked_machine_id(" 7f3e9c2a-1b4d-4e8f-9a0c-d5e6f7a8b9c0 ").unwrap(),
            "7f3e9c2a-1b4d-4e8f-9a0c-d5e6f7a8b9c0"
        );
    }

    #[test]
    fn load_missing_returns_default() {
        let s = SourceState::load(Path::new("/nonexistent/trnovel/xyz.json"));
        assert_eq!(s, SourceState::default());
    }

    #[test]
    fn ttl_purges_expired_login() {
        let mut s = SourceState::default();
        s.login_header
            .insert("Authorization".into(), "Bearer x".into());
        s.cookies.insert("site.com".into(), "sid=1".into());
        s.kv.insert("keep".into(), "me".into());

        // 未来过期:不动。
        s.expire_at = Some(now_secs() + 3600);
        assert!(!s.is_login_expired());
        assert!(!s.purge_if_expired());
        assert!(!s.login_header.is_empty());

        // 已过期:清登录态、保留 kv。
        s.expire_at = Some(now_secs().saturating_sub(1));
        assert!(s.is_login_expired());
        assert!(s.purge_if_expired());
        assert!(s.login_header.is_empty());
        assert!(s.login_info.is_none());
        assert!(s.cookies.is_empty());
        assert_eq!(s.expire_at, None);
        assert_eq!(
            s.kv.get("keep").map(String::as_str),
            Some("me"),
            "kv 应保留"
        );
    }

    #[cfg(unix)]
    #[test]
    fn save_writes_0600_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join("trnovel-state-perm-test");
        let path = dir.join("s.json");
        // 预置一个 0644 的旧文件,验证 save 会收紧已存在文件。
        let _ = std::fs::create_dir_all(&dir);
        let _ = std::fs::write(&path, "{}");
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644));
        let mut s = SourceState::default();
        s.login_header
            .insert("Authorization".into(), "Bearer x".into());
        s.save(&path).unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "凭据状态文件应以 0600 落盘");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn save_then_load_file() {
        let dir = std::env::temp_dir().join("trnovel-state-test");
        let path = dir.join("s.json");
        let mut s = SourceState::default();
        s.kv.insert("k".into(), "v".into());
        s.save(&path).unwrap();
        let back = SourceState::load(&path);
        assert_eq!(back.kv.get("k").map(String::as_str), Some("v"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
