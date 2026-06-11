//! JS host 桥的对象注册与 native 函数:把 `source`(状态/登录)与 `net`(网络/cookie/浏览器)
//! 两个有状态对象注入 boa `Context`;native 函数经 thread-local 取当前 [`SourceHost`] 执行。

use super::*;

/// 注入 `source`(状态/登录)与 `net`(网络/cookie/浏览器)两个对象。
pub(super) fn register_host(ctx: &mut Context) -> JsResult<()> {
    // source:书源 per-source 状态(跨请求 KV、单槽变量)+ 登录态(loginHeader 明文 / loginInfo 密文)。
    let source = ObjectInitializer::new(ctx)
        .function(NativeFunction::from_fn_ptr(js_put), js_string!("put"), 2)
        .function(NativeFunction::from_fn_ptr(js_get), js_string!("get"), 1)
        .function(
            NativeFunction::from_fn_ptr(js_get_variable),
            js_string!("getVariable"),
            0,
        )
        .function(
            NativeFunction::from_fn_ptr(js_put_variable),
            js_string!("putVariable"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(js_put_login_header),
            js_string!("putLoginHeader"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(js_get_login_header),
            js_string!("getLoginHeader"),
            0,
        )
        .function(
            NativeFunction::from_fn_ptr(js_get_login_header_map),
            js_string!("getLoginHeaderMap"),
            0,
        )
        .function(
            NativeFunction::from_fn_ptr(js_remove_login_header),
            js_string!("removeLoginHeader"),
            0,
        )
        .function(
            NativeFunction::from_fn_ptr(js_put_login_info),
            js_string!("putLoginInfo"),
            1,
        )
        .function(
            NativeFunction::from_fn_ptr(js_get_login_info),
            js_string!("getLoginInfo"),
            0,
        )
        .function(
            NativeFunction::from_fn_ptr(js_get_login_info_map),
            js_string!("getLoginInfoMap"),
            0,
        )
        .build();
    ctx.register_global_property(js_string!("source"), source, Attribute::all())?;

    // net:网络出口与 cookie 读取(startBrowserAwait 由后续任务补全 7.x)。
    let net = ObjectInitializer::new(ctx)
        .function(NativeFunction::from_fn_ptr(js_ajax), js_string!("ajax"), 1)
        .function(
            NativeFunction::from_fn_ptr(js_connect),
            js_string!("connect"),
            1,
        )
        .function(NativeFunction::from_fn_ptr(js_post), js_string!("post"), 2)
        .function(
            NativeFunction::from_fn_ptr(js_get_cookie),
            js_string!("getCookie"),
            2,
        )
        .build();
    ctx.register_global_property(js_string!("net"), net, Attribute::all())?;
    Ok(())
}

// ───────────────────────── source.* native 函数 ─────────────────────────

/// `source.put(key, value)`:写入跨请求 KV,返回 value。
fn js_put(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let key = arg(args, 0, ctx)?;
    let value = arg(args, 1, ctx)?;
    if let Some(host) = active_host() {
        let mut h = host.borrow_mut();
        h.state.kv.insert(key, value.clone());
        h.dirty = true;
    }
    Ok(js_string!(value.as_str()).into())
}

/// `source.get(key)`:读取 KV,缺失返回空串(不抛错)。
fn js_get(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let key = arg(args, 0, ctx)?;
    let v = active_host()
        .map(|h| h.borrow().state.kv.get(&key).cloned().unwrap_or_default())
        .unwrap_or_default();
    Ok(js_string!(v.as_str()).into())
}

/// `source.getVariable()`:读取书源级单槽变量。
fn js_get_variable(_t: &JsValue, _args: &[JsValue], _ctx: &mut Context) -> JsResult<JsValue> {
    let v = active_host()
        .map(|h| h.borrow().state.variable.clone())
        .unwrap_or_default();
    Ok(js_string!(v.as_str()).into())
}

/// `source.putVariable(value)`:写入书源级单槽变量,返回 value。
fn js_put_variable(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let value = arg(args, 0, ctx)?;
    if let Some(host) = active_host() {
        let mut h = host.borrow_mut();
        h.state.variable = value.clone();
        h.dirty = true;
    }
    Ok(js_string!(value.as_str()).into())
}

// ── 登录态:loginHeader(明文 header map)与 loginInfo(加密凭据)──

/// 由字符串 map 构造一个 JS 对象 `{k: v, ...}`。
fn map_to_js_object(map: &BTreeMap<String, String>, ctx: &mut Context) -> JsObject {
    let mut init = ObjectInitializer::new(ctx);
    for (k, v) in map {
        init.property(
            js_string!(k.as_str()),
            js_string!(v.as_str()),
            Attribute::all(),
        );
    }
    init.build()
}

/// `source.putLoginHeader(json)`:用 JSON 对象**整体设置**登录态请求头(JWT/自定义头/Cookie 均可),
/// 含 `Cookie` 字段时同步并入 cookie 库;返回原 json;非法 JSON 抛 JS 异常。
fn js_put_login_header(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let json = arg(args, 0, ctx)?;
    let r = json_to_string_map(&json).map(|map| {
        if let Some(host) = active_host() {
            host.borrow_mut().set_login_header(map);
        }
        json.clone()
    });
    yield_js(r)
}

/// `source.getLoginHeader()`:返回登录态请求头的 JSON 字符串(为空返回空串)。
fn js_get_login_header(_t: &JsValue, _args: &[JsValue], _ctx: &mut Context) -> JsResult<JsValue> {
    let s = active_host()
        .map(|h| {
            let h = h.borrow();
            if h.state.login_header.is_empty() {
                String::new()
            } else {
                serde_json::to_string(&h.state.login_header).unwrap_or_default()
            }
        })
        .unwrap_or_default();
    Ok(js_string!(s.as_str()).into())
}

/// `source.getLoginHeaderMap()`:返回登录态请求头的 JS 对象。
fn js_get_login_header_map(
    _t: &JsValue,
    _args: &[JsValue],
    ctx: &mut Context,
) -> JsResult<JsValue> {
    let map = active_host()
        .map(|h| h.borrow().state.login_header.clone())
        .unwrap_or_default();
    Ok(map_to_js_object(&map, ctx).into())
}

/// `source.removeLoginHeader()`:清空登录态请求头。
fn js_remove_login_header(
    _t: &JsValue,
    _args: &[JsValue],
    _ctx: &mut Context,
) -> JsResult<JsValue> {
    if let Some(host) = active_host() {
        let mut h = host.borrow_mut();
        if !h.state.login_header.is_empty() {
            h.state.login_header.clear();
            h.dirty = true;
        }
    }
    Ok(JsValue::undefined())
}

/// `source.putLoginInfo(plain)`:加密存储登录凭据(机器绑定密钥),返回原文;失败抛 JS 异常。
fn js_put_login_info(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let plain = arg(args, 0, ctx)?;
    let r = match active_host() {
        Some(host) => host
            .borrow_mut()
            .store_login_info(&plain)
            .map(|_| plain.clone()),
        None => Ok(plain.clone()),
    };
    yield_js(r)
}

/// `source.getLoginInfo()`:解密返回登录凭据明文(未设置返回空串);失败抛 JS 异常。
fn js_get_login_info(_t: &JsValue, _args: &[JsValue], _ctx: &mut Context) -> JsResult<JsValue> {
    let r = match active_host() {
        Some(host) => host
            .borrow()
            .state
            .get_login_info()
            .map(Option::unwrap_or_default),
        None => Ok(String::new()),
    };
    yield_js(r)
}

/// `source.getLoginInfoMap()`:解密凭据并解析为 JS 对象(未设置返回空对象);失败抛 JS 异常。
fn js_get_login_info_map(_t: &JsValue, _args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let plain = match active_host() {
        Some(host) => match host.borrow().state.get_login_info() {
            Ok(o) => o.unwrap_or_default(),
            Err(e) => return Err(JsNativeError::typ().with_message(e.to_string()).into()),
        },
        None => String::new(),
    };
    if plain.is_empty() {
        return Ok(ObjectInitializer::new(ctx).build().into());
    }
    let map =
        json_to_string_map(&plain).map_err(|e| JsNativeError::typ().with_message(e.to_string()))?;
    Ok(map_to_js_object(&map, ctx).into())
}

// ───────────────────────── net.* native 函数 ─────────────────────────

/// 取第 `i` 个参数作为可选「额外请求头 JSON 串」:JS 对象会被 to_string 成
/// `"[object Object]"`,该值与空串都视为无额外头(headers 须经 JSON 串传入)。
fn opt_extra_arg(args: &[JsValue], i: usize, ctx: &mut Context) -> JsResult<Option<String>> {
    let extra = arg(args, i, ctx)?;
    Ok((!extra.is_empty() && extra != "[object Object]").then_some(extra))
}

/// 把请求结果转为 JS 值:成功构造 `{body, code, headers}` 对象,失败抛可被 `try/catch`
/// 捕获的 JS 异常(connect/post 共用收尾)。
fn yield_response(r: Result<FetchResponse, EvalError>, ctx: &mut Context) -> JsResult<JsValue> {
    match r {
        Ok(resp) => Ok(response_to_js(&resp, ctx).into()),
        Err(e) => Err(JsNativeError::typ().with_message(e.to_string()).into()),
    }
}

// 注:net.* 一律 `borrow_mut`(请求核心要把响应 Set-Cookie 写回 state.cookies);
// 网络调用是同线程同步 block_on,期间不会重入 JS,无 RefCell 双借风险。

/// `net.ajax(url)`:复用取页管线发 GET,返回响应体;失败抛 JS 异常(可 `try/catch`)。
fn js_ajax(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let url = arg(args, 0, ctx)?;
    let r = match active_host() {
        Some(host) => host.borrow_mut().ajax(&url),
        None => Err(EvalError::Host("no active host".into())),
    };
    yield_js(r)
}

/// `net.connect(url, extraHeadersJson?)`:发 GET 返回完整响应对象 `{body, code, headers}`
/// (供读 `Set-Cookie`/`Location`/状态码);失败抛 JS 异常。第二参为可选的额外请求头 JSON 串。
fn js_connect(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let url = arg(args, 0, ctx)?;
    let extra = opt_extra_arg(args, 1, ctx)?;
    let r = match active_host() {
        Some(host) => host.borrow_mut().connect(&url, extra.as_deref()),
        None => Err(EvalError::Host("no active host".into())),
    };
    yield_response(r, ctx)
}

/// `net.post(url, body, extraHeadersJson?)`:发 POST 返回完整响应对象 `{body, code, headers}`
/// (供表单/JSON 登录);失败抛 JS 异常。第三参为可选的额外请求头 JSON 串。
fn js_post(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let url = arg(args, 0, ctx)?;
    let body = arg(args, 1, ctx)?;
    let extra = opt_extra_arg(args, 2, ctx)?;
    let r = match active_host() {
        Some(host) => host.borrow_mut().post(&url, &body, extra.as_deref()),
        None => Err(EvalError::Host("no active host".into())),
    };
    yield_response(r, ctx)
}

/// 把 [`FetchResponse`] 构造成 JS 对象 `{body: string, code: number, headers: {k:v}}`。
fn response_to_js(resp: &FetchResponse, ctx: &mut Context) -> JsObject {
    let headers: BTreeMap<String, String> = resp
        .headers
        .iter()
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let headers_obj = map_to_js_object(&headers, ctx);
    ObjectInitializer::new(ctx)
        .property(
            js_string!("body"),
            js_string!(resp.body.as_str()),
            Attribute::all(),
        )
        .property(
            js_string!("code"),
            JsValue::from(i32::from(resp.status)),
            Attribute::all(),
        )
        .property(js_string!("headers"), headers_obj, Attribute::all())
        .build()
}

/// `net.getCookie(domain, key?)`:读 cookie 库;给定 key 取该 cookie 值,否则返回整串。
fn js_get_cookie(_t: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    let domain = arg(args, 0, ctx)?;
    let key = arg(args, 1, ctx)?;
    let v = active_host()
        .map(|h| {
            h.borrow()
                .get_cookie(&domain, (!key.is_empty()).then_some(key.as_str()))
        })
        .unwrap_or_default();
    Ok(js_string!(v.as_str()).into())
}
