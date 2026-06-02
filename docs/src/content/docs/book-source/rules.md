---
title: 规则语法
sidebar:
    order: 3
lastUpdated: 2026-06-02
---

书源里几乎每个字段(书名、URL、章节列表……)都是一条**规则**。规则要么是一次**抽取(叶子)**,要么是一个**组合子**。本页是规则的完整语法。

## 叶子规则:一次抽取

```json
{ "via": "css", "select": "h3 a", "extract": "text" }
```

| 字段 | 说明 |
|------|------|
| `via` | 抽取后端:`css`(默认)/ `xpath`(XPath 1.0)/ `json`(JSONPath)/ `regex`(fancy-regex)/ `raw`(直接用上下文值,不选择) |
| `select` | 选择器:CSS 选择器 / JSONPath / 正则;`via: "raw"` 时省略。**HTML 的 `select` 是 self-or-descendant 语义**(省略 `select` = 元素自身) |
| `index` | 取第 N 个匹配(值规则),负数从末尾 |
| `extract` | 取值方式,见下 |
| `clean` | 有序后处理流水线,见下 |

### extract —— 取什么值

- `"text"`(默认)、`"ownText"`、`"html"`、`"innerHtml"`、`"outerHtml"`
- 取属性:`{ "attr": "href" }`(也常用 `src` / `data-src` / `content`)

```json
{ "via": "css", "select": "a",   "extract": { "attr": "href" } }   // 取链接
{ "via": "css", "select": ".c", "extract": "html" }                // 标签转换行后清理
{ "via": "json", "select": "$.url_list" }                          // JSONPath
```

### clean —— 后处理流水线

按顺序执行,每步用其中一种:

```json
"clean": [
  { "regex": "请收藏本站[^<\\n]*", "replace": "" },   // 正则替换(replace 默认空串)
  { "trim": true },                                   // 去首尾空白
  { "prepend": "https://example.com" },               // 前缀
  { "append": "?nocache=1" }                          // 后缀
]
```

除上面四种外,`clean` 还支持 `decode`/`encode`/`hash`/`cipher`/`cn` 等**加解密 / 编码 / 哈希 / 繁简**算子(用于反爬解密、签名),详见下方「加解密 / 编码 / 哈希」小节。

## 组合子

| 组合子 | 作用 | 例 |
|--------|------|----|
| `firstOf` | 返回**首个非空**子规则结果(回退 / 兼容多结构) | `{ "firstOf": [ {规则A}, {规则B} ] }` |
| `concat` | **拼接**非空子规则结果,可选 `join` | `{ "concat": [ {A}, {B} ], "join": " · " }` |
| `literal` | 字面量值 | `{ "literal": "玄幻" }` |
| `template` | 模板插值 | `{ "template": "{{base}}/s?q={{key}}" }` |

```json
// 章节名:优先取 .module-row-title,没有则退回 h2
"name": { "firstOf": [
  { "via": "css", "select": ".module-row-title", "extract": "text" },
  { "via": "css", "select": "h2", "extract": "text" }
] }
```

## URL 与模板变量

URL 字段(`search.request.url`、`explore.categories[].url` 等)可以是**字符串模板**或一条规则:

```json
"url": { "template": "{{base}}/search.html?searchkey={{key}}" }
```

内置变量:

| 变量 | 含义 |
|------|------|
| `{{base}}` | 站点根(顶层 `url`,已去尾斜杠) |
| `{{key}}` | 搜索词 |
| `{{page}}` | 当前页码 |
| `{{pageSize}}` | 每页条数 |

请求级 `vars`(命名捕获)也可在模板中引用。相对 URL 引擎会自动补全成绝对地址,**抽取 `href` 时保留相对路径即可**。

## 后端选择

- **`css`**(默认):用 [dom_query](https://crates.io/crates/dom_query) 的 CSS 选择器,支持 `:has()`/`:contains()` 等扩展伪类。
- **`json`**:目标是 JSON 接口时用 JSONPath(如 `$[*]` / `$.url_list`)。
- **`regex`**:fancy-regex(支持前后向断言)。
- **`raw`**:直接使用上下文当前值,常用于「列表项本身就是目标值」。
- **`xpath`**:XPath 1.0(纯 Rust)。脏 HTML 会被宽松解析后求值;表达式自带取值意图时(`//a/@href`、`//p/text()`、`concat(...)`)直接取标量、忽略 `extract`,否则按 `index`+`extract` 取元素。**对良构度的要求高于 CSS,脏站建议优先用 CSS**,XPath 作层级/轴选择(祖先、兄弟、按位置)的补充。

```json
{ "via": "xpath", "select": "//div[@class='content']//p", "extract": "text" }
{ "via": "xpath", "select": "//a[@class='chapter']/@href" }   // 标量,忽略 extract
```

## 加解密 / 编码 / 哈希(clean 算子)

反爬常把正文 Base64/AES 加密、把 URL 用 MD5 签名。这些是**确定性变换**,直接作为 `clean` 步表达,**无需 JS**。每步按固定顺序执行:`regex → trim → prepend → append → decode → encode → hash → cipher → cn`。

| 算子 | 说明 |
|------|------|
| `decode` / `encode` | `base64` / `base64url` / `hex` / `url`(百分号) |
| `hash` | `{ algo: md5\|sha1\|sha256\|sha512, output: hex\|base64, hmacKey?, hmacKeyEnc? }` |
| `cipher` | 对称加解密(见下) |
| `cn` | 繁简:`t2s`(繁→简) / `s2t`(简→繁) |

```json
// 正文:Base64 解码后 AES-CBC 解密(cipher 默认 op=decrypt、inputEnc=base64、outputEnc=utf8)
"value": { "via": "css", "select": ".content", "clean": [
  { "decode": "base64" },
  { "cipher": { "algo": "aes", "mode": "cbc", "key": "0123456789abcdef", "iv": "abcdef9876543210" } }
] }

// 签名 URL:md5(页码+salt),全程结构化、不碰 JS
"url": { "concat": [
  { "template": "{{base}}/c?p={{page}}&s=" },
  { "template": "{{page}}salt", "clean": [ { "hash": { "algo": "md5" } } ] }
] }
```

`cipher` 字段:`algo`(`aes`/`des`/`tripleDes`)、`mode`(`cbc`/`ecb`/`cfb`/`gcm`)、`padding`(`pkcs7`默认/`zero`/`none`)、`op`(`decrypt`默认/`encrypt`)、`key`、`keyEnc`(`utf8`默认/`base64`/`hex`)、`iv`/`ivEnc`、`inputEnc`/`outputEnc`。默认值贴合「解密正文」主场景。密钥/密文出错会**显式报错**(便于定位),不静默返回空。

## JS 逃生舱(可选)

绝大多数书源用上面的结构化规则 + crypto 算子即可。**只有少数需要运行时逻辑编排**(条件、循环、把多个结果按逻辑拼接、时间戳签名)的场景才用 JS。

```json
{ "js": "var t = Date.now(); baseUrl + '/api?t=' + t + '&sign=' + crypto.md5(t + 'salt')" }
```

也可作 `clean` 的一步(`result` = 当前串):`{ "clean": [ { "js": "result.split('|')[1]" } ] }`。

脚本里可用:只读变量 `result`(当前上下文)、`baseUrl`、`key`/`page`/…;以及 `crypto` 助手(`md5`/`sha1`/`sha256`/`sha512`/`base64Encode`/`base64Decode`/`hexEncode`/`hexDecode`/`aesEncrypt(data,key,iv)`/`aesDecrypt(data,key,iv)`/`t2s`/`s2t`,后端与上面的 crypto 算子同源)。

:::note
JS 求值需构建时启用 `js` feature(官方分发的二进制已启用)。`Rule::Js` 始终可被解析与 schema 校验;未启用 `js` 的构建在求值到 JS 时会报「不支持」。**结构化规则优先,JS 仅作逃生舱。**
:::

完整字段与取值约束以自动生成的 [`book-source.schema.json`](https://github.com/yexiyue/TRNovel/blob/main/crates/parse-book-source/book-source.schema.json) 为准。
