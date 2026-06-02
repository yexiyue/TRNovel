//! JS 逻辑编排逃生舱(`js` feature)。纯 Rust JS 引擎(boa),同步求值。
//!
//! 注入只读全局:`result`(当前上下文)、`baseUrl`、以及各模板变量(`key`/`page`/…);
//! 另注入 `crypto` 助手对象——其方法**直接复用 [`crate::transform`] 的纯 Rust crypto fn**
//! (单一真相源,非 `java`)。无任何网络/文件/宿主能力,攻击面极小(见 design D4/D5)。

use crate::error::EvalError;
use crate::eval::Vars;
use crate::source::{
    ByteEnc, CipherAlgo, CipherMode, CipherOp, CipherStep, CnConvert, Codec, HashAlgo, HashOut,
    HashStep,
};
use crate::transform;
use boa_engine::object::ObjectInitializer;
use boa_engine::property::Attribute;
use boa_engine::{
    Context, JsError, JsNativeError, JsResult, JsValue, NativeFunction, Source, js_string,
};

/// 求值一段 JS:以 `result` 为当前上下文,注入变量与 `crypto` 助手,返回字符串结果。
pub fn eval_js(script: &str, result: &str, vars: &Vars) -> Result<String, EvalError> {
    let mut ctx = Context::default();
    register(&mut ctx, result, vars).map_err(to_eval)?;
    let value = ctx.eval(Source::from_bytes(script)).map_err(to_eval)?;
    Ok(value
        .to_string(&mut ctx)
        .map_err(to_eval)?
        .to_std_string_escaped())
}

fn to_eval(e: JsError) -> EvalError {
    EvalError::Js(e.to_string())
}

/// 注入全局绑定与 `crypto` 对象。
fn register(ctx: &mut Context, result: &str, vars: &Vars) -> JsResult<()> {
    ctx.register_global_property(js_string!("result"), js_string!(result), Attribute::all())?;
    if let Some(base) = vars.get("base") {
        ctx.register_global_property(
            js_string!("baseUrl"),
            js_string!(base.as_str()),
            Attribute::all(),
        )?;
    }
    for (k, v) in vars {
        ctx.register_global_property(
            js_string!(k.as_str()),
            js_string!(v.as_str()),
            Attribute::all(),
        )?;
    }
    let crypto = ObjectInitializer::new(ctx)
        .function(NativeFunction::from_fn_ptr(js_md5), js_string!("md5"), 1)
        .function(NativeFunction::from_fn_ptr(js_sha1), js_string!("sha1"), 1)
        .function(
            NativeFunction::from_fn_ptr(js_sha256),
            js_string!("sha256"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(js_sha512),
            js_string!("sha512"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(js_base64_encode),
            js_string!("base64Encode"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(js_base64_decode),
            js_string!("base64Decode"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(js_hex_encode),
            js_string!("hexEncode"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(js_hex_decode),
            js_string!("hexDecode"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(js_aes_decrypt),
            js_string!("aesDecrypt"),
            3,
        )
        .function(
            NativeFunction::from_fn_ptr(js_aes_encrypt),
            js_string!("aesEncrypt"),
            3,
        )
        .function(NativeFunction::from_fn_ptr(js_t2s), js_string!("t2s"), 1)
        .function(NativeFunction::from_fn_ptr(js_s2t), js_string!("s2t"), 1)
        .build();
    ctx.register_global_property(js_string!("crypto"), crypto, Attribute::all())?;
    Ok(())
}

// ───────────────────────── 原生函数(后端复用 transform)─────────────────────────

/// 取第 `i` 个参数为字符串(缺省空串)。
fn arg(args: &[JsValue], i: usize, ctx: &mut Context) -> JsResult<String> {
    match args.get(i) {
        Some(v) => Ok(v.to_string(ctx)?.to_std_string_escaped()),
        None => Ok(String::new()),
    }
}

/// `Result<String, EvalError>` → JS 返回值 / 抛错。
fn yield_js(r: Result<String, EvalError>) -> JsResult<JsValue> {
    match r {
        Ok(s) => Ok(js_string!(s.as_str()).into()),
        Err(e) => Err(JsNativeError::typ().with_message(e.to_string()).into()),
    }
}

macro_rules! hash_fn {
    ($name:ident, $algo:expr) => {
        fn $name(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
            let s = arg(args, 0, ctx)?;
            yield_js(transform::hash(
                &s,
                &HashStep {
                    algo: $algo,
                    output: HashOut::Hex,
                    hmac_key: None,
                    hmac_key_enc: ByteEnc::Utf8,
                },
            ))
        }
    };
}
hash_fn!(js_md5, HashAlgo::Md5);
hash_fn!(js_sha1, HashAlgo::Sha1);
hash_fn!(js_sha256, HashAlgo::Sha256);
hash_fn!(js_sha512, HashAlgo::Sha512);

macro_rules! codec_fn {
    ($name:ident, $f:path, $codec:expr) => {
        fn $name(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
            let s = arg(args, 0, ctx)?;
            yield_js($f(&s, $codec))
        }
    };
}
codec_fn!(js_base64_encode, transform::encode, Codec::Base64);
codec_fn!(js_base64_decode, transform::decode, Codec::Base64);
codec_fn!(js_hex_encode, transform::encode, Codec::Hex);
codec_fn!(js_hex_decode, transform::decode, Codec::Hex);

/// `aesDecrypt(data, key, iv)`:AES-CBC/PKCS7,key/iv 按 utf8,密文 base64→明文 utf8。
fn js_aes_decrypt(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let data = arg(args, 0, ctx)?;
    let step = aes_cbc_step(arg(args, 1, ctx)?, arg(args, 2, ctx)?, CipherOp::Decrypt);
    yield_js(transform::cipher(&data, &step))
}

/// `aesEncrypt(data, key, iv)`:明文 utf8→密文 base64。
fn js_aes_encrypt(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let data = arg(args, 0, ctx)?;
    let step = aes_cbc_step(arg(args, 1, ctx)?, arg(args, 2, ctx)?, CipherOp::Encrypt);
    yield_js(transform::cipher(&data, &step))
}

fn aes_cbc_step(key: String, iv: String, op: CipherOp) -> CipherStep {
    let (input_enc, output_enc) = match op {
        CipherOp::Decrypt => (ByteEnc::Base64, ByteEnc::Utf8),
        CipherOp::Encrypt => (ByteEnc::Utf8, ByteEnc::Base64),
    };
    CipherStep {
        algo: CipherAlgo::Aes,
        mode: CipherMode::Cbc,
        padding: crate::source::Padding::Pkcs7,
        op,
        key,
        key_enc: ByteEnc::Utf8,
        iv: Some(iv),
        iv_enc: ByteEnc::Utf8,
        input_enc: Some(input_enc),
        output_enc: Some(output_enc),
    }
}

fn js_t2s(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let s = arg(args, 0, ctx)?;
    Ok(js_string!(transform::cn_convert(&s, CnConvert::T2s).as_str()).into())
}

fn js_s2t(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let s = arg(args, 0, ctx)?;
    Ok(js_string!(transform::cn_convert(&s, CnConvert::S2t).as_str()).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Vars {
        let mut v = Vars::new();
        v.insert("base".into(), "https://x.com".into());
        v.insert("page".into(), "3".into());
        v
    }

    #[test]
    fn js_uses_result_and_vars() {
        let out = eval_js("result + '|' + baseUrl + '|' + page", "CTX", &vars()).unwrap();
        assert_eq!(out, "CTX|https://x.com|3");
    }

    #[test]
    fn js_control_flow_builds_url() {
        let out = eval_js(
            "var n = parseInt(page) * 10; baseUrl + '/list/' + n + '.html'",
            "",
            &vars(),
        )
        .unwrap();
        assert_eq!(out, "https://x.com/list/30.html");
    }

    #[test]
    fn crypto_md5_matches_transform() {
        let out = eval_js("crypto.md5('hello')", "", &Vars::new()).unwrap();
        assert_eq!(out, "5d41402abc4b2a76b9719d911017c592");
    }

    #[test]
    fn crypto_aes_roundtrip_in_js() {
        // 用 crypto 对象加密再解密,验证后端就是 transform。
        let out = eval_js(
            "var ct = crypto.aesEncrypt('正文', '0123456789abcdef', 'abcdef9876543210'); \
             crypto.aesDecrypt(ct, '0123456789abcdef', 'abcdef9876543210')",
            "",
            &Vars::new(),
        )
        .unwrap();
        assert_eq!(out, "正文");
    }

    #[test]
    fn crypto_base64_and_t2s() {
        assert_eq!(
            eval_js("crypto.base64Decode('aGVsbG8=')", "", &Vars::new()).unwrap(),
            "hello"
        );
        assert_eq!(
            eval_js("crypto.t2s('漢字')", "", &Vars::new()).unwrap(),
            "汉字"
        );
    }

    #[test]
    fn js_runtime_error_surfaces() {
        let r = eval_js("throw new Error('boom')", "", &Vars::new());
        assert!(matches!(r, Err(EvalError::Js(_))));
    }
}
