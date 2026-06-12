---
title: 书源结构
sidebar:
    order: 2
lastUpdated: 2026-06-02
---

本页讲清楚每个能力块长什么样。每个字段的取值是一条「规则」,规则的写法见[规则语法](/TRNovel/book-source/rules)。完整约束以 [`book-source.schema.json`](https://github.com/yexiyue/TRNovel/blob/main/crates/parse-book-source/book-source.schema.json)(由 Rust 类型自动生成)为准。

## http —— 请求配置

```json
"http": {
  "headers":  { "User-Agent": "Mozilla/5.0 …", "Referer": "https://example.com/" },
  "cookies":  { "key": "value" },
  "warmup":   ["https://example.com/"],
  "charset":  "auto",
  "fetcher":  "auto",
  "timeout":  15000,
  "retry":    { "max": 2, "backoffMs": 500 },
  "rateLimit":{ "maxCount": 1, "perMs": 1000 }
}
```

| 字段 | 说明 |
|------|------|
| `headers` | 每个请求都带的请求头(常用于 `User-Agent` / `Referer`) |
| `cookies` | 静态 cookie;也是手动注入 `cf_clearance` 等通行证的落点 |
| `warmup` | 先 GET 这些页以预热会话 cookie(站点需要先访问首页/某接口才放行时用) |
| `charset` | `auto`(默认,UTF-8 失败回退 GBK)/ `utf8` / `gbk` / `gb18030` / `big5` |
| `fetcher` | 取页模式:`auto`(默认,撞反爬才用浏览器)/ `reqwest`(永不开浏览器)/ `browser`(整站走浏览器)。见[反爬](/TRNovel/reference/anti-scraping) |
| `timeout` | 单请求超时(毫秒) |
| `retry` | 失败重试:`max` 次数、`backoffMs` 退避 |
| `rateLimit` | 限速:`perMs` 毫秒内最多 `maxCount` 次 |

## search —— 搜索

```json
"search": {
  "request": { "url": { "template": "{{base}}/search.html?searchkey={{key}}" }, "method": "GET" },
  "list":    { "via": "css", "select": ".module-search-item" },
  "item": {
    "bookUrl": { "via": "css", "select": "h3 a", "extract": { "attr": "href" } },
    "name":    { "via": "css", "select": "h3 a", "extract": "text" },
    "cover":   { "via": "css", "select": ".module-item-pic img", "extract": { "attr": "data-src" } }
  }
}
```

- `request`:`url`(模板或规则)、`method`(`GET`/`POST`)、可选 `body`、可选 `headers`、可选 `vars`(命名捕获供模板用)。`{{key}}` = 搜索词。
- `list`:选中**所有结果条目**的规则。
- `item`:在每个条目上抽字段(同 `bookInfo` 的字段集),**必须含 `bookUrl`**(指向书详情)。

## explore —— 分类浏览

两阶段:`entries` 生成可选择的**入口**,`page` 用选中入口的**变量**取一页书。入口身份是「标题 + 变量」而非固定 URL——取页 URL 由 `page.request` 用入口变量与 `{{page}}` 模板生成。

```json
"explore": {
  "entries": [
    { "static": [ { "title": "玄幻", "vars": { "cat": "1" } } ] }
  ],
  "page": {
    "request": { "url": { "template": "{{base}}/sort/{{cat}}_{{page}}.html" } },
    "list": { "via": "css", "select": ".module-item" },
    "item": { "bookUrl": { "…": "…" }, "name": { "…": "…" } }
  }
}
```

- `entries`:**入口源数组**,按声明顺序合并(没有独立的 chain 类型——「按序合并」就是数组本身)。每个源是:
  - `static`:固定入口列表,每项 `title` + 可选 `vars`(字面量取页变量)。
  - `fetch`:请求远端分类数据动态生成入口——`request`(复用 search 的请求形态)、`list` 抽数据项、`item.title`/`item.vars` 生成入口;可选 `forEach` 用多组变量重复请求并合并。`item` 规则可同时读当前数据项(`via:json` 等)与外层 `forEach` 变量(`{{name}}`)。
- `page`:与 `search` **同构的列表页规格**(`prelude`/`request`/`list`/`item`,`render`/`interceptApi`/`totalPages`/`hasMore`/`pageBy` 都在 `request` 上)。选中入口变量 + `page`/`pageSize` 合并后驱动它取页。

搜索被反爬挡住时,浏览是重要的降级入口。

## bookInfo —— 书详情

在书详情页抽取字段(均可省略):

| 字段 | 含义 |
|------|------|
| `name` | 书名 |
| `author` | 作者 |
| `cover` | 封面图 URL |
| `intro` | 简介 |
| `kind` | 分类 / 标签 / 状态 |
| `lastChapter` | 最新章节名 |
| `tocUrl` | 目录页 URL(常来自 `og:novel:read_url`) |
| `wordCount` | 字数 |

很多站点提供 `og:` meta,最稳:

```json
"bookInfo": {
  "name":   { "via": "css", "select": "[property=\"og:novel:book_name\"]", "extract": { "attr": "content" } },
  "tocUrl": { "via": "css", "select": "[property=\"og:novel:read_url\"]",  "extract": { "attr": "content" } }
}
```

## toc —— 目录(章节 + 分卷)

```json
"toc": {
  "list":     { "via": "css", "select": ".box > h2.module-title, .box a.module-row-text" },
  "name":     { "firstOf": [ { "via":"css","select":".module-row-title","extract":"text" },
                             { "via":"css","select":"h2","extract":"text" } ] },
  "url":      { "via": "css", "select": "a", "extract": { "attr": "href" } },
  "isVolume": { "via": "css", "select": "h2", "extract": "text" },
  "maxPages": 1
}
```

- `list` 同时选中**卷标题**与**章节链接**并保持文档顺序;`isVolume` 对卷节点求值非空、对章节为空 → 引擎据此切分「卷 → 章」生成折叠目录树。无分卷时省略 `isVolume`。
- 目录翻页:可选 `nextPage`(选下一页链接的规则)+ `maxPages`(硬上限,默认 1)。

## content —— 正文

```json
"content": {
  "value": {
    "via": "css", "select": ".article-content", "extract": "html",
    "clean": [ { "regex": "请收藏本站[^<\\n]*", "replace": "" }, { "trim": true } ]
  },
  "maxPages": 1
}
```

- `value`:正文抽取规则。`extract: "html"` 会把 `<p>`/`<br>` 转成换行再清理标签;`clean` 流水线去掉页脚广告等。
- 正文分页:可选 `nextPage` + `maxPages`。

## samples —— 黄金样例

```json
"samples": [
  { "bookUrl": "/novel/guzhenren.html",
    "expect": { "name": "蛊真人", "volumes": 8, "minChapters": 2000, "minContentChars": 500 } }
]
```

`samples` 提供一两本真实书的期望(`name` / `minChapters` / `volumes` / `minContentChars`),供 `trn doctor` 全流程校验,也用于运行期自愈。**强烈建议至少写一条**。
