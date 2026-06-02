---
title: 介绍
sidebar:
    order: 1
lastUpdated: 2026-06-02
---

书源是 TRNovel 接入网络小说的方式:一份**声明式 JSON**,告诉引擎如何在某个网站上**搜索、浏览分类、读取书籍信息、列出目录(含分卷)、抓取正文**。

## 设计:结构化、对 AI 友好

TRNovel 的书源是 `trnovel-booksource/v2` 格式。和传统的「紧凑字符串 DSL」(如 `[property=og:novel:book_name]@content##简介:`)不同,v2 里**每个字段都是一个结构化的「规则」对象**:

```json
// 旧式紧凑 DSL(不再使用)
"name": "[property=og:novel:book_name]@content"

// v2 结构化规则
"name": { "via": "css", "select": "[property=\"og:novel:book_name\"]", "extract": { "attr": "content" } }
```

这样做的好处:

- **可读、可校验**:字段含义一目了然,JSON Schema 能静态约束。
- **对 AI 友好**:结构清晰,AI 能可靠地生成与修改 —— 配套的 `booksource-generator` skill 可以**据网站 URL 自动生成书源**(见[制作书源](/TRNovel/book-source/make))。
- **schema 永不漂移**:`book-source.schema.json` 由 Rust 类型**自动生成**,与引擎行为始终一致。

## 顶层结构一览

| 字段 | 必填 | 说明 |
|------|:---:|------|
| `schema` | ✅ | 固定为 `"trnovel-booksource/v2"` |
| `name` | ✅ | 书源名称 |
| `url` | ✅ | 站点根地址(模板里的 `{{base}}`) |
| `group` | | 分组(便于管理) |
| `http` | | 请求配置:headers / cookies / warmup / charset / 取页模式 / 超时 / 重试 / 限速 |
| `search` | | 搜索能力 |
| `explore` | | 分类浏览能力 |
| `bookInfo` | ✅ | 书详情页的字段抽取 |
| `toc` | ✅ | 目录(章节 + 分卷) |
| `content` | ✅ | 正文抽取 |
| `samples` | | 黄金样例,驱动 `doctor` 校验与运行期自愈 |

字段详解见[书源结构](/TRNovel/book-source/structure),规则语法见[规则语法](/TRNovel/book-source/rules)。

## 最小示例

```json title="minimal.v2.json"
{
  "schema": "trnovel-booksource/v2",
  "name": "示例书源",
  "url": "https://example.com",
  "bookInfo": {
    "name": { "via": "css", "select": "h1.book-title", "extract": "text" },
    "tocUrl": { "via": "css", "select": "a.catalog", "extract": { "attr": "href" } }
  },
  "toc": {
    "list": { "via": "css", "select": ".chapter-list a" },
    "name": { "via": "raw" },
    "url": { "via": "css", "select": "a", "extract": { "attr": "href" } }
  },
  "content": {
    "value": { "via": "css", "select": "#content", "extract": "html" }
  }
}
```

## 从配置到可用

```text
据网站逆向/AI 生成 → trn doctor 校验 → trn import 导入 → trn -n 选用
```

- 生成与逆向技巧 → [制作书源](/TRNovel/book-source/make)
- 反爬(Cloudflare 等)的处理 → [反爬与浏览器辅助](/TRNovel/reference/anti-scraping)
- 命令 → [CLI 参考](/TRNovel/reference/cli)(`doctor` / `import`)

:::tip
书源是网络爬虫配置,可能因网站结构变化而失效。`samples` 里写上一两本书的期望,`trn doctor` 就能随时体检书源是否仍然有效。
:::
