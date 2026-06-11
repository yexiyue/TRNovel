//! v2 书源配置类型(纯 serde,镜像 `book-source.schema.json`)。
//!
//! 规则是显式结构化对象,无任何紧凑字符串 DSL。`Rule` 既是配置、也是供求值器
//! 遍历的语法树(见 design D1/D6)。配置类型按职责分文件:
//! - [`rule`] — 规则 AST(`Rule`/`LeafRule`/抽取语义)。
//! - [`clean`] — `clean` 流水线算子(编解码/哈希/加解密/繁简)。
//! - [`http`] — HTTP 配置与请求/多步编排(前置链 + 命名捕获)。
//! - [`op`] — 各操作(搜索/浏览/详情/目录/正文)的字段规则与样例。
//!
//! 本文件聚合顶层 [`BookSource`] 并 re-export 上述子模块类型,使 `crate::source::*`
//! 历史路径保持不变。

pub mod clean;
pub mod http;
pub mod op;
pub mod rule;

pub use clean::*;
pub use http::*;
pub use op::*;
pub use rule::*;

use crate::error::ConfigError;
use serde::{Deserialize, Serialize};

/// v2 书源。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "schema", derive(schemars::JsonSchema))]
pub struct BookSource {
    /// 固定为 `"trnovel-booksource/v2"`。
    pub schema: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub group: String,
    /// 站点基址,用于相对链接解析与 `{{base}}`。
    pub url: String,
    #[serde(default)]
    pub http: Http,
    /// 登录:普通 URL,或登录脚本(`@js:…` / `<js>…</js>` 包裹,内含 `login()` 函数)。
    /// 非空即视为「需要登录」(见 [`BookSource::has_login`]);仅 `js-host` 构建可真正执行脚本登录。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub login_url: String,
    /// 声明式登录表单(TUI 渲染);收集值加密存为 loginInfo,供 `login()` 脚本读取。
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub login_ui: Vec<RowUi>,
    /// 登录态过期校验脚本:每个网络方法响应后执行(注入 `result`=响应),判失效可提示重登。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub login_check_js: String,
    /// 开启后:响应的 `Set-Cookie` 自动回灌进 cookie 库(按注册域归并持久化)。
    #[serde(default)]
    pub enabled_cookie_jar: bool,
    /// 限速:`"N/ms"`(N 次/ms)或纯毫秒间隔字符串;为空则用 `http.rateLimit`。
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub concurrent_rate: String,
    /// 命名共享 fontMap 表(`名字 -> {码点:真字}`):供多个 clean 步骤按名复用,避免同一张
    /// 大表在书源里重复内联。clean 的 `fontMap` 写字符串名(如 `{"fontMap":"search"}`)即引用本表,
    /// 在 [`BookSource::from_json`] 解析时就地展开为内联表(运行期与直接内联等价)。
    #[serde(default, skip_serializing_if = "std::collections::BTreeMap::is_empty")]
    pub font_maps: std::collections::BTreeMap<String, std::collections::BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub search: Option<SearchOp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explore: Option<ExploreOp>,
    pub book_info: BookInfoOp,
    pub toc: TocRules,
    pub content: ContentRules,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub samples: Vec<Sample>,
}

/// 期望的 schema 标识。
pub const SCHEMA_ID: &str = "trnovel-booksource/v2";

/// 递归展开命名共享 fontMap:遇到对象里 `"fontMap"` 的值为**字符串**(引用名)时,
/// 用 `registry[名]` 的内联表替换;然后递归子节点(含刚内联的表,内层无 fontMap 故安全)。
fn expand_named_font_maps(v: &mut serde_json::Value, registry: &serde_json::Value) {
    match v {
        serde_json::Value::Object(map) => {
            let replacement = match map.get("fontMap") {
                Some(serde_json::Value::String(name)) => registry.get(name).cloned(),
                _ => None,
            };
            if let Some(table) = replacement {
                map.insert("fontMap".to_string(), table);
            }
            for val in map.values_mut() {
                expand_named_font_maps(val, registry);
            }
        }
        serde_json::Value::Array(arr) => {
            for val in arr.iter_mut() {
                expand_named_font_maps(val, registry);
            }
        }
        _ => {}
    }
}

impl BookSource {
    /// 从 JSON 字符串解析一个书源。
    ///
    /// 解析前先展开**命名共享 fontMap**:把所有 clean 步骤里 `{"fontMap":"名字"}` 的字符串引用,
    /// 就地替换为顶层 `fontMaps[名字]` 的内联表(通用 JSON 遍历,覆盖任意位置;未知名→留原字符串,
    /// 后续按 BTreeMap 反序列化会失败并给出清晰错误)。展开后与「直接内联」逐字节等价。
    pub fn from_json(s: &str) -> Result<Self, ConfigError> {
        let mut v: serde_json::Value = serde_json::from_str(s)?;
        if let Some(registry) = v.get("fontMaps").cloned()
            && registry.is_object()
        {
            expand_named_font_maps(&mut v, &registry);
        }
        Ok(serde_json::from_value(v)?)
    }

    /// 是否需要登录(`loginUrl` 或 `loginUi` 任一非空)。据此在 TUI 暴露「书源登录」入口。
    /// 注:仅配置 `loginUi` 而无登录脚本/`loginUrl` 属配置不完整,由登录页拦截并提示。
    pub fn has_login(&self) -> bool {
        !self.login_url.trim().is_empty() || !self.login_ui.is_empty()
    }

    /// 若 `loginUrl` 是登录脚本(`@js:` 或 `<js>…</js>` 包裹),剥壳返回脚本体;
    /// 否则(普通 URL 或空)返回 `None`——此时走 headful 浏览器登录。
    pub fn get_login_js(&self) -> Option<&str> {
        let s = self.login_url.trim();
        if let Some(js) = s.strip_prefix("@js:") {
            Some(js.trim())
        } else if let Some(rest) = s.strip_prefix("<js>") {
            Some(rest.strip_suffix("</js>").unwrap_or(rest).trim())
        } else {
            None
        }
    }

    /// 从 JSON 值解析一个或多个书源(支持单对象或数组)。
    /// 与 [`BookSource::from_json`] 一致:逐源就地展开命名 fontMap 引用(数组里每个源各自的
    /// `fontMaps` 注册表),否则 `{"fontMap":"名"}` 字符串引用会反序列化失败。
    pub fn from_value_many(value: serde_json::Value) -> Result<Vec<Self>, ConfigError> {
        fn expand_one(v: &mut serde_json::Value) {
            if let Some(reg) = v.get("fontMaps").cloned()
                && reg.is_object()
            {
                expand_named_font_maps(v, &reg);
            }
        }
        match value {
            serde_json::Value::Array(mut arr) => {
                for item in arr.iter_mut() {
                    expand_one(item);
                }
                Ok(serde_json::from_value(serde_json::Value::Array(arr))?)
            }
            mut v => {
                expand_one(&mut v);
                Ok(vec![serde_json::from_value(v)?])
            }
        }
    }

    /// 从本地文件导入(支持单对象或数组)。
    pub fn from_path(path: &str) -> Result<Vec<Self>, crate::error::BookSourceError> {
        let text = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
        let value = serde_json::from_str(&text).map_err(ConfigError::Json)?;
        Ok(Self::from_value_many(value)?)
    }

    /// 从网络 URL 导入(支持单对象或数组)。
    pub async fn from_url(url: &str) -> Result<Vec<Self>, crate::error::BookSourceError> {
        use crate::error::FetchError;
        let text = reqwest::get(url)
            .await
            .map_err(FetchError::Http)?
            // 先判 HTTP 状态:4xx/5xx 返回错误页时,避免把"非 JSON"误报成 JSON 解析失败。
            .error_for_status()
            .map_err(FetchError::Http)?
            .text()
            .await
            .map_err(FetchError::Http)?;
        let value = serde_json::from_str(&text).map_err(ConfigError::Json)?;
        Ok(Self::from_value_many(value)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 与 examples/bilixs.v2.json 同构的代表性书源(覆盖 leaf / firstOf / concat /
    /// template / attr / http+cookies / samples / 分卷 isVolume)。
    const BILIXS_V2: &str = r#"{
      "schema": "trnovel-booksource/v2",
      "name": "哔哩小说",
      "group": "测试",
      "url": "https://www.bilixs.com",
      "http": {
        "headers": { "User-Agent": "Mozilla/5.0" },
        "cookies": {},
        "warmup": ["https://www.bilixs.com/"],
        "charset": "auto",
        "timeout": 15000,
        "retry": { "max": 2, "backoffMs": 500 }
      },
      "search": {
        "request": { "url": { "template": "{{base}}/search.html?searchkey={{key}}" }, "method": "GET" },
        "list": { "via": "css", "select": ".module-item" },
        "item": {
          "name": { "via": "css", "select": ".module-item-title", "extract": "text" },
          "tocUrl": { "via": "css", "select": ".module-item-title", "extract": { "attr": "href" } }
        }
      },
      "explore": {
        "categories": [ { "title": "最近更新", "url": { "template": "{{base}}/book/lastupdate_0_1_0_0_0_0_0_{{page}}_0.html" } } ],
        "list": { "via": "css", "select": ".module-item" },
        "item": { "name": { "via": "css", "select": ".module-item-title", "extract": "text" } }
      },
      "bookInfo": {
        "name": { "via": "css", "select": "[property=\"og:novel:book_name\"]", "extract": { "attr": "content" } },
        "cover": { "via": "css", "select": "[property=\"og:image\"]", "extract": { "attr": "content" } },
        "kind": { "concat": [
            { "via": "css", "select": "[property=\"og:novel:tags\"]", "extract": { "attr": "content" } },
            { "via": "css", "select": "[property=\"og:novel:status\"]", "extract": { "attr": "content" } }
          ], "join": " · " },
        "tocUrl": { "via": "css", "select": "[property=\"og:novel:read_url\"]", "extract": { "attr": "content" } }
      },
      "toc": {
        "list": { "via": "css", "select": ".box > h2.module-title.type, .box a.module-row-text" },
        "name": { "firstOf": [
            { "via": "css", "select": ".module-row-title", "extract": "text" },
            { "via": "css", "select": "h2", "extract": "text" }
          ] },
        "url": { "via": "css", "select": "a", "extract": { "attr": "href" } },
        "isVolume": { "via": "css", "select": "h2", "extract": "text" },
        "maxPages": 1
      },
      "content": {
        "value": { "via": "css", "select": ".article-content", "extract": "html",
          "clean": [ { "regex": "请收藏本站[^<\\n]*", "replace": "" }, { "trim": true } ] }
      },
      "samples": [
        { "bookUrl": "/novel/guzhenren.html", "expect": { "name": "蛊真人", "volumes": 8, "minChapters": 2000 } }
      ]
    }"#;

    #[test]
    fn parses_v2_book_source() {
        let bs = BookSource::from_json(BILIXS_V2).expect("应解析 v2 书源");
        assert_eq!(bs.schema, SCHEMA_ID);
        assert_eq!(bs.name, "哔哩小说");
    }

    #[test]
    fn named_font_map_is_expanded_from_registry() {
        // clean 里 `{"fontMap":"fm"}` 字符串引用 → 顶层 fontMaps["fm"] 内联表;另留一份直接内联对照。
        let json = r#"{
            "schema":"trnovel-booksource/v2","name":"t","url":"https://e.com",
            "fontMaps": { "fm": { "E001": "一", "E002": "二" } },
            "bookInfo": { "name": {"via":"css","select":"h1"} },
            "toc": { "list": {"via":"css","select":"a"}, "name": {"via":"css","select":"a"},
                     "url": {"via":"css","select":"a","extract":{"attr":"href"}} },
            "content": { "value": {"via":"css","select":".c","extract":"html",
                         "clean":[{"fontMap":"fm"}]} }
        }"#;
        let bs = BookSource::from_json(json).expect("应解析并展开命名 fontMap");
        let clean = match &bs.content.value {
            Rule::Leaf(l) => &l.clean,
            other => panic!("content.value 应为叶子: {other:?}"),
        };
        let fm = clean[0]
            .font_map
            .as_ref()
            .expect("命名 fontMap 应展开为内联表");
        assert_eq!(fm.get("E001").map(String::as_str), Some("一"));
        assert_eq!(fm.get("E002").map(String::as_str), Some("二"));
    }

    #[test]
    fn inline_font_map_still_works() {
        // 直接内联的 fontMap(无命名引用)保持原样,向后兼容。
        let json = r#"{
            "schema":"trnovel-booksource/v2","name":"t","url":"https://e.com",
            "bookInfo": { "name": {"via":"css","select":"h1"} },
            "toc": { "list": {"via":"css","select":"a"}, "name": {"via":"css","select":"a"},
                     "url": {"via":"css","select":"a","extract":{"attr":"href"}} },
            "content": { "value": {"via":"css","select":".c","extract":"html",
                         "clean":[{"fontMap":{"E0FF":"好"}}]} }
        }"#;
        let bs = BookSource::from_json(json).unwrap();
        let Rule::Leaf(l) = &bs.content.value else {
            panic!("应为叶子")
        };
        assert_eq!(
            l.clean[0]
                .font_map
                .as_ref()
                .unwrap()
                .get("E0FF")
                .map(String::as_str),
            Some("好")
        );
    }

    #[test]
    fn toc_name_is_firstof_with_two_leaves() {
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        match &bs.toc.name {
            Rule::FirstOf { first_of } => assert_eq!(first_of.len(), 2),
            other => panic!("toc.name 应为 firstOf,实际 {other:?}"),
        }
    }

    #[test]
    fn toc_is_volume_is_leaf_css_h2() {
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        let iv = bs.toc.is_volume.as_ref().expect("isVolume 应存在");
        match iv {
            Rule::Leaf(l) => {
                assert_eq!(l.via, Via::Css);
                assert_eq!(l.select.as_deref(), Some("h2"));
            }
            other => panic!("isVolume 应为叶子,实际 {other:?}"),
        }
    }

    #[test]
    fn search_url_is_template_rule() {
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        let req = &bs.search.as_ref().unwrap().request;
        match &req.url {
            UrlOrRule::Rule(r) => assert!(matches!(**r, Rule::Template { .. })),
            other => panic!("search.request.url 应为模板规则,实际 {other:?}"),
        }
    }

    #[test]
    fn book_info_cover_extracts_attr() {
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        match bs.book_info.cover.as_ref().unwrap() {
            Rule::Leaf(l) => assert_eq!(
                l.extract,
                Extract::Attr {
                    attr: "content".into()
                }
            ),
            other => panic!("cover 应为属性抽取叶子,实际 {other:?}"),
        }
    }

    #[test]
    fn http_cookies_and_warmup_parsed() {
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        assert_eq!(bs.http.warmup, vec!["https://www.bilixs.com/"]);
        assert_eq!(bs.http.charset, Charset::Auto);
        assert_eq!(bs.http.retry.as_ref().unwrap().backoff_ms, 500);
    }

    #[test]
    fn sample_expectations_parsed() {
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        let s = &bs.samples[0];
        assert_eq!(s.expect.volumes, Some(8));
        assert_eq!(s.expect.min_chapters, Some(2000));
    }

    #[test]
    fn round_trips_through_json() {
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        let json = serde_json::to_string(&bs).unwrap();
        let bs2 = BookSource::from_json(&json).unwrap();
        assert_eq!(bs, bs2);
    }

    #[test]
    fn rejects_unknown_top_level_field() {
        let bad = BILIXS_V2.replacen("\"name\":", "\"nmae\":", 1);
        assert!(
            BookSource::from_json(&bad).is_err(),
            "拼错字段应被 deny_unknown_fields 拒绝"
        );
    }

    // ── 审查/test-coverage:has_login 判定(决定 TUI 是否显示登录入口)──
    #[test]
    fn has_login_when_login_url_or_login_ui_present() {
        let mut bs = BookSource::from_json(BILIXS_V2).unwrap();
        assert!(!bs.has_login(), "默认无 loginUrl/loginUi 不需登录");
        bs.login_url = "https://site/login".into();
        assert!(bs.has_login());
        bs.login_url = "@js:function login(){}".into();
        assert!(bs.has_login());
        bs.login_url = "   ".into();
        assert!(!bs.has_login(), "纯空白 loginUrl 视为不需登录");
        // 仅配置 loginUi 也计入(TUI 须给登录入口;配置完整性由登录页校验)。
        bs.login_ui = vec![RowUi {
            name: "用户名".into(),
            ..Default::default()
        }];
        assert!(bs.has_login(), "loginUi 非空应计入登录入口判定");
    }

    // ── 审查/test-coverage:get_login_js 剥壳各分支(@js: / <js>…</js> / 半包裹 / 普通 URL / 空)──
    #[test]
    fn get_login_js_strips_prefixes() {
        let mut bs = BookSource::from_json(BILIXS_V2).unwrap();
        bs.login_url = "@js: function login(){} ".into();
        assert_eq!(bs.get_login_js(), Some("function login(){}"));
        bs.login_url = "<js>BODY</js>".into();
        assert_eq!(bs.get_login_js(), Some("BODY"));
        bs.login_url = "<js> A </js>".into();
        assert_eq!(bs.get_login_js(), Some("A"));
        bs.login_url = "<js>BODY".into(); // 缺尾标签:容错保留整段
        assert_eq!(bs.get_login_js(), Some("BODY"));
        bs.login_url = "https://site/login".into(); // 普通 URL → None(走浏览器登录)
        assert_eq!(bs.get_login_js(), None);
        bs.login_url = "".into();
        assert_eq!(bs.get_login_js(), None);
    }

    // ── 11.1:前置请求链 + 结构化捕获 解析 / round-trip / deny_unknown_fields ──
    #[test]
    fn parses_prelude_capture_and_round_trips() {
        let json = r#"{
          "schema":"trnovel-booksource/v2","name":"t","url":"https://x",
          "search":{
            "prelude":[{
              "url":{"template":"{{base}}/prepare"},
              "capture":[{"name":"token","value":{"via":"json","select":"$.token"},"scope":"source"}],
              "skipIfPresent":["token"]
            }],
            "request":{"url":{"template":"{{base}}/s?token={{token}}"}},
            "list":{"via":"css","select":".i"},
            "item":{"name":{"via":"css","select":".t","extract":"text"}}
          },
          "bookInfo":{"prelude":[{"url":{"template":"{{base}}/p"},"capture":[{"name":"csrf","value":{"via":"raw"}}]}]},
          "toc":{"list":{"via":"css","select":"a"},"name":{"via":"css","select":"a"},"url":{"via":"css","select":"a","extract":{"attr":"href"}}},
          "content":{"value":{"via":"css","select":".c"}}
        }"#;
        let bs = BookSource::from_json(json).expect("应解析含 prelude 的书源");
        let sp = &bs.search.as_ref().unwrap().prelude;
        assert_eq!(sp.len(), 1);
        assert_eq!(sp[0].capture[0].name, "token");
        assert_eq!(sp[0].capture[0].scope, VarScope::Source);
        assert_eq!(sp[0].skip_if_present, vec!["token".to_string()]);
        // bookInfo 前置步骤默认 scope = chapter。
        assert_eq!(bs.book_info.prelude[0].capture[0].scope, VarScope::Chapter);
        // round-trip 相等。
        let s = serde_json::to_string(&bs).unwrap();
        assert_eq!(BookSource::from_json(&s).unwrap(), bs);
    }

    #[test]
    fn prestep_rejects_unknown_field() {
        let bad = r#"{
          "schema":"trnovel-booksource/v2","name":"t","url":"https://x",
          "toc":{"prelude":[{"url":{"template":"{{base}}/p"},"captuer":[]}],
                 "list":{"via":"css","select":"a"},"name":{"via":"css","select":"a"},"url":{"via":"css","select":"a"}},
          "bookInfo":{},
          "content":{"value":{"via":"css","select":".c"}}
        }"#;
        assert!(
            BookSource::from_json(bad).is_err(),
            "PreStep 拼错字段(captuer)应被 deny_unknown_fields 拒"
        );
    }

    #[test]
    fn existing_source_serializes_without_new_fields() {
        // 向后兼容:无 prelude/vars 的书源序列化输出不含任何新字段(逐字节)。
        let bs = BookSource::from_json(BILIXS_V2).unwrap();
        let json = serde_json::to_string(&bs).unwrap();
        assert!(!json.contains("prelude"), "无前置链不应序列化 prelude");
        assert!(!json.contains("\"vars\""), "空 vars 不应序列化");
        assert!(!json.contains("skipIfPresent"));
        assert!(!json.contains("\"capture\""));
    }
}

/// 防漂移:`book-source.schema.json` 必须等于从类型现生成的 schema(`--features schema`)。
/// 失败说明改了配置类型却没重新生成 schema——按提示重跑 gen_schema 即可。
#[cfg(all(test, feature = "schema"))]
mod schema_sync {
    #[test]
    fn schema_is_in_sync() {
        let generated =
            serde_json::to_string_pretty(&schemars::schema_for!(crate::BookSource)).unwrap();
        let committed = include_str!("../../book-source.schema.json");
        assert_eq!(
            generated.trim(),
            committed.trim(),
            "book-source.schema.json 与配置类型不同步;请重新生成:\n  \
             cargo run -p parse-book-source --features schema --example gen_schema \
             > crates/parse-book-source/book-source.schema.json"
        );
    }
}
