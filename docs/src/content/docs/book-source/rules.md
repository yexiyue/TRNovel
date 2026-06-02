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
| `via` | 抽取后端:`css`(默认)/ `xpath` / `json`(JSONPath)/ `regex`(fancy-regex)/ `raw`(直接用上下文值,不选择) |
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
- `xpath`:占位,暂以 css 为主。

完整字段与取值约束以自动生成的 [`book-source.schema.json`](https://github.com/yexiyue/TRNovel/blob/main/crates/parse-book-source/book-source.schema.json) 为准。
