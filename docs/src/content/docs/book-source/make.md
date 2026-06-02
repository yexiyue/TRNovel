---
title: 制作书源
sidebar:
    order: 4
lastUpdated: 2026-06-02
---

import { Steps, Tabs, TabItem } from '@astrojs/starlight/components';

做一个书源有两条路:**让 AI 做**(推荐,最省事)或**手动逆向**。两者最后都用 `trn doctor` 校验、`trn import` 导入。

## 方式一:让 AI 做(推荐)

仓库内置 `booksource-generator` skill(在 `skills/` 下,可用 [`npx skills`](https://github.com/vercel-labs/agent-skills) 安装)。把它装给你的 AI 助手,然后:

<Steps>
1. 把小说站 URL 交给 AI:「给这个站做个 trnovel 书源 https://…」;
2. AI 探站、逆向出 search / 浏览 / 书详情 / 目录(含分卷)/ 正文 的选择器,并自动用 `trn doctor` 全流程校验、按 ✓/✗ 迭代到通过(连字符集、反爬都会处理);
3. AI 跑 `trn import` 导入 —— 立刻在 `trn -n` 里读到这个站。
</Steps>

skill 内含规则 schema、真实示例与逆向战术手册,生成的书源是结构化 JSON,质量可由 `doctor` 客观保证。

## 方式二:手动逆向

<Steps>
1. **探站**:抓取代表性页面看结构 —— 首页(找搜索表单)、一本书的详情页、它的目录页、一个搜索结果页、一个分类页。

   ```bash
   UA='Mozilla/5.0 … Chrome/124.0 Safari/537.36'
   curl -s -A "$UA" "https://example.com/book/123/" -o page.html
   grep -oiE 'property="og:[^"]*"|class="[^"]*"' page.html | sort | uniq -c | sort -rn | head
   ```

2. **逐能力写规则**(语法见[规则语法](/TRNovel/book-source/rules),形态见[书源结构](/TRNovel/book-source/structure)):
   - **bookInfo**:优先 `og:` meta(`og:novel:book_name` / `og:novel:read_url` / `og:image` …),最稳。
   - **toc**:用一个 `list` 同时选中卷标题与章节链接,`isVolume` 区分二者,引擎据此切「卷 → 章」。
   - **content**:定位正文容器(`.article-content` / `#content` …),`extract: "html"`,必要时 `clean` 去页脚。
   - **search**:从首页 `<form>` 找 action 与参数名;`item` 必须含 `bookUrl`。

3. **判定字符集**:中文乱码就设 `http.charset`(`utf8` / `gbk`);不确定用默认 `auto`。

4. **校验 → 导入**(见下)。
</Steps>

### 常见坑

- **目录开头的「最新章节」预览**:很多目录页前 N 条是倒序、与正文区重复的预览。用兄弟选择器(如 `#list dl dt:has(a[href*="txt_"]) ~ dd a`)只取正文区之后的章节。
- **POST 搜索**:不少站需显式带 `"headers": { "Content-Type": "application/x-www-form-urlencoded" }` 才返回结果。
- **反爬**:撞 Cloudflare 见[反爬与浏览器辅助](/TRNovel/reference/anti-scraping);先找未被挑战的等价入口,再考虑 `fetcher` 模式。

## 校验:`trn doctor`

对书源跑完整流程,逐项报告 ✓/✗/○,**一次只修一个 ✗ 对应的规则,重跑直到全绿**:

```bash
trn doctor my-source.v2.json
```

```text
书源诊断:哔哩小说
  ✓ 配置     书源「哔哩小说」
  ✓ 浏览     20 本(分类「最近更新」)
  ✓ 书详情    蛊真人
  ✓ 目录     8 卷 / 2336 章
  ✓ 正文     2402 字
  ✓ 搜索     「蛊真人」→ 16 本
```

无 `samples` 时,doctor 会用浏览结果探一本书来测读取链路,所以先把 `explore` 或 `samples` 做对能解锁后面几项。

## 导入:`trn import`

校验通过后导入,使其在网络小说里可选用(按 `url`+`name` 去重,同名覆盖):

```bash
trn import my-source.v2.json     # 本地文件
trn import https://…/source.json # 也支持 URL
trn -n                            # 在网络小说里选用
```

## 完整示例

下面是哔哩小说(bilixs)的完整 v2 书源,覆盖 search / explore / bookInfo / toc(分卷)/ content / samples,可作为模板:

```json title="bilixs.v2.json"
{
  "schema": "trnovel-booksource/v2",
  "name": "哔哩小说",
  "group": "测试",
  "url": "https://www.bilixs.com",
  "http": {
    "headers": {
      "User-Agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36",
      "Referer": "https://www.bilixs.com/"
    },
    "warmup": ["https://www.bilixs.com/"],
    "charset": "auto",
    "timeout": 15000,
    "retry": { "max": 2, "backoffMs": 500 }
  },
  "search": {
    "request": { "url": { "template": "{{base}}/search.html?searchkey={{key}}" }, "method": "GET" },
    "list": { "via": "css", "select": ".module-search-item" },
    "item": {
      "bookUrl": { "via": "css", "select": "h3 a", "extract": { "attr": "href" } },
      "name": { "via": "css", "select": "h3 a", "extract": "text" },
      "cover": { "via": "css", "select": ".module-item-pic img", "extract": { "attr": "data-src" } }
    }
  },
  "explore": {
    "categories": [
      { "title": "最近更新", "url": { "template": "{{base}}/book/lastupdate_0_1_0_0_0_0_0_{{page}}_0.html" } }
    ],
    "list": { "via": "css", "select": ".module-item" },
    "item": {
      "name": { "via": "css", "select": ".module-item-title", "extract": "text" },
      "tocUrl": { "via": "css", "select": ".module-item-title", "extract": { "attr": "href" } }
    }
  },
  "bookInfo": {
    "name": { "via": "css", "select": "[property=\"og:novel:book_name\"]", "extract": { "attr": "content" } },
    "author": { "via": "css", "select": "[property=\"og:novel:author\"]", "extract": { "attr": "content" } },
    "cover": { "via": "css", "select": "[property=\"og:image\"]", "extract": { "attr": "content" } },
    "intro": { "via": "css", "select": "[property=\"og:description\"]", "extract": { "attr": "content" } },
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
    "value": {
      "via": "css", "select": ".article-content", "extract": "html",
      "clean": [ { "regex": "请收藏本站[^<\\n]*", "replace": "" }, { "trim": true } ]
    },
    "maxPages": 1
  },
  "samples": [
    { "bookUrl": "/novel/guzhenren.html",
      "expect": { "name": "蛊真人", "volumes": 8, "minChapters": 2000, "minContentChars": 500 } }
  ]
}
```
