---
title: 介绍
sidebar:
    order: 1
lastUpdated: 2025-10-29
---

## 什么是书源？

书源是 TRNovel 获取网络小说内容的重要方式。 它们定义了如何从不同的网站抓取小说章节和元数据。通过使用书源，TRNovel 能够支持多种小说网站，让用户可以方便地阅读和管理他们喜欢的小说。书源的本质是一个JSON 爬虫配置文件，描述了如何从特定网站提取小说内容。

## 示例

```json title="笔趣阁书源示例.json"
{
    "bookSourceGroup": "精品",
    "bookSourceName": "笔趣阁",
    "bookSourceUrl": "https://www.4c7f720b2.lol",
    "lastUpdateTime": 1736150932184,
    "searchUrl": "https://www.4c7f720b2.lol/user/hm.html?q={{key}},https://www.4c7f720b2.lol/user/search.html?q={{key}}&so=undefined",
    "exploreUrl": "[{\"title\":\"玄幻\",\"url\":\"https://www.4c7f720b2.lol/json?sortid=1&page={{page}}\"},{\"title\":\"武侠\",\"url\":\"https://www.4c7f720b2.lol/json?sortid=2&page={{page}}\"},{\"title\":\"都市\",\"url\":\"https://www.4c7f720b2.lol/json?sortid=3&page={{page}}\"},{\"title\":\"历史\",\"url\":\"https://www.4c7f720b2.lol/json?sortid=4&page={{page}}\"},{\"title\":\"网游\",\"url\":\"https://www.4c7f720b2.lol/json?sortid=5&page={{page}}\"},{\"title\":\"科幻\",\"url\":\"https://www.4c7f720b2.lol/json?sortid=6&page={{page}}\"},{\"title\":\"女生\",\"url\":\"https://www.4c7f720b2.lol/json?sortid=7&page={{page}}\"},{\"title\":\"完本\",\"url\":\"https://www.4c7f720b2.lol/json?sortid=8&page={{page}}\"}]",
    "ruleExploreItem": null,
    "header": null,
    "respondTime": null,
    "httpConfig": {
        "timeout": null,
        "header": null,
        "rateLimit": null
    },
    "ruleBookInfo": {
        "name": "[property=og:novel:book_name]@content",
        "author": "[property=og:novel:author]@content",
        "coverUrl": "[property=og:image]@content",
        "intro": "[property=og:description]@content##简介：",
        "kind": "[property=og:novel:update_time]@content&&\n[property=og:novel:category]@content&&\n[property=og:novel:status]@content",
        "lastChapter": "[property=og:novel:latest_chapter_name]@content",
        "tocUrl": "",
        "wordCount": ""
    },
    "ruleContent": {
        "content": "id.chaptercontent@html##请收藏本站.*|<a.*</a>|第(.*?)页|\\[爱豆看书\\]|ｍ．２６ｋｓw.ｃｃ"
    },
    "ruleExplore": {
        "bookList": "$.*",
        "bookUrl": "$.url_list",
        "name": "$.articlename",
        "author": "$.author",
        "coverUrl": "$.url_img",
        "intro": "$.intro",
        "kind": "",
        "lastChapter": "",
        "tocUrl": "",
        "wordCount": ""
    },
    "ruleSearch": {
        "bookList": "$.*",
        "bookUrl": "$.url_list",
        "name": "$.articlename",
        "author": "$.author",
        "coverUrl": "$.url_img",
        "intro": "$.intro",
        "kind": "",
        "lastChapter": "",
        "tocUrl": "",
        "wordCount": ""
    },
    "ruleToc": {
        "chapterList": "@css:.listmain dd a[href^=\"/\"]",
        "chapterName": "tag.a@text",
        "chapterUrl": "tag.a@href"
    }
}
```

:::tip[提示]
由于是网络爬虫配置文件，书源可能会因为网站结构变化而失效。建议定期更新书源以确保其有效性。
非常欢迎用户贡献和分享新的书源配置文件，以丰富 TRNovel 的书源库。
:::
