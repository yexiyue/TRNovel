# 笔趣阁 bqgui.cc 书源 — 结论

## doctor 校验结果(`trn doctor bqgui.v2.json`)
```
✓ 配置     书源「笔趣阁(bqgui)」
✓ 浏览     15 本(分类「玄幻」)
✓ 书详情    斗罗大陆V重生唐三
✓ 目录     0 卷 / 1184 章
✗ 正文     正文为空
○ 搜索     未配置
```

## 能用的部分
- **浏览(explore)**:8 个分类(玄幻/武侠/都市/历史/网游/科幻/女生/完本),静态页 `.item` 列表,书名/作者/封面/简介都抽得到。
- **书详情(bookInfo)**:`/book/<id>/` 页有完整 `og:novel:*` meta(书名、作者、封面、简介、分类+状态、最新章节、tocUrl),稳。
- **目录(toc)**:同在 `/book/<id>/` 页,`.listmain dl dd a` 1184 章准确抽出,无分卷(扁平列表)。

→ 这意味着可以在 TRNovel 里浏览分类、看书籍信息、列出完整章节目录。

## 不能用的部分(站点反爬,非规则 bug)
- **正文(content)✗**:章节页 `/book/<id>/<n>.html` 会 302 跳到 `/user/verify.html`(一个 "加载中……" 的 JS 壳),用 favicon 探测设置 `getsite` cookie,再跳到轮换镜像域名(如 `bqg128.xyz`)的 **SPA**。镜像把正文渲染进 `<div id="read">`,通过 `read.js` 调 `/api/hm?hash=MD5(...)` + `/api/action`(CryptoJS 签名)动态拉取并客户端解码。**没有任何静态 HTTP 路径能拿到正文文本**。TRNovel 的 reqwest 取页拿不到;`fetcher:"browser"` 也救不了——它只是 Cloudflare 挑战求解器(签发 cf_clearance 后交回 reqwest),不会执行站点的内容加载 JS 并返回渲染后的 DOM。
- **搜索(search)○ 未配置**:搜索框 `<form action="/s">` 是 JS 驱动。结果走 JSON 接口 `/user/search.html?q=<词>`,但**必须先按同一关键词调 `/user/hm.html?q=<词>`**(它按词设一个 md5 的 `hm` cookie,用别的词的 cookie 无效),且响应被 Cloudflare 缓存、要加 `&_=<时间戳>` 破缓存才返回结果。验证过该接口确实按关键词过滤(斗罗大陆/完美世界 返回各自相关结果)。这是「按关键词的两步 AJAX + 破缓存」流程,而 v2 引擎的 search 只发**单个请求**(warmup 只支持静态 URL,不带关键词),无法表达,故留空而非配一个会一直失败的规则。

## 如果想真正读正文
需要给 TRNovel 增加「能执行页面 JS 的渲染型 fetcher」(真无头浏览器跑完 read.js 后取 `#read` 文本),当前 browser feature 只解 CF 挑战,做不到。本书源的正文规则保留了 `#content`/`#chaptercontent`/`.content`/`#read` 的 firstOf 兜底,一旦站点改回静态正文即可自动生效。
