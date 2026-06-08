# 逆向站点 → 书源:战术手册

配合 `SKILL.md` 的核心循环使用。本文给出规则语法速查、探站 curl 配方、逐能力逆向细节、字符集与反爬处理。精确字段以 `book-source.schema.json` 为准,完整范例见 `example-bilixs.v2.json`。

## 目录
- [规则语法速查](#规则语法速查)
- [探站配方](#探站配方)
- [按能力逆向](#按能力逆向)
- [字符集](#字符集)
- [反爬](#反爬)
- [常见 CMS 模式](#常见-cms-模式)
- [doctor 迭代](#doctor-迭代)

## 规则语法速查

一条「规则」是下列之一(`oneOf`):

```jsonc
// 叶子:在当前上下文做一次抽取
{ "via": "css", "select": "h3 a", "extract": "text" }
{ "via": "css", "select": "a", "extract": { "attr": "href" } }      // 取属性
{ "via": "css", "select": ".content", "extract": "html" }            // 标签转换行后清理
{ "via": "json", "select": "$.data[*].name" }                        // JSONPath(接口返回 JSON 时)
{ "via": "regex", "select": "chapter_(\\d+)" }                        // 正则(fancy-regex)
{ "via": "raw" }                                                     // 直接用上下文值,不选择

// 组合子
{ "firstOf": [ {规则A}, {规则B} ] }          // 返回首个非空结果(回退/兼容多结构)
{ "concat": [ {规则A}, {规则B} ], "join": " " } // 拼接非空结果
{ "literal": "玄幻" }                          // 字面量
{ "template": "{{base}}/search?wd={{key}}" }   // 插值
```

要点:
- `via` 默认 `css`;HTML 的 `select` 是 **self-or-descendant**(省略 `select` = 元素自身)。
- `extract`:`"text"` | `"ownText"` | `"html"` | `"innerHtml"` | `"outerHtml"` | `{"attr":"href"}`(默认 `text`)。文本类会 trim。
- `index`:取第 N 个匹配(值规则),负数从末尾。
- `clean`:有序后处理流水线,每步可含多个算子(按固定序执行):`{"regex":"…","replace":"…"}`、`{"trim":true}`、`{"prepend"/"append":"…"}`、`{"decode":"base64"}`/`{"encode":…}`、`{"hash":{…}}`、`{"cipher":{…}}`(AES/DES 解密)、`{"fontMap":{"E4DE":"一",…}}`(字体反爬还原,见下)、`{"cn":"t2s"}`(繁简转换)。
- `urlOrRule`:字符串模板(支持 `{{base}}/{{key}}/{{page}}/{{pageSize}}` 与请求级 `vars`)或一条规则。
- 列表类操作(search/explore):`list` 选中**所有结果条目**,`item`(bookRules)在**每个条目**上抽字段。

## 探站配方

带浏览器 UA + Referer 抓页并存盘(老站会按 UA 区别对待):

```bash
UA='Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0 Safari/537.36'
curl -s -A "$UA" -H 'Referer: https://例子.com/' --max-time 20 "<url>" -o /tmp/page.html
# 状态码 + 反爬头:
curl -s -o /dev/null -D - -A "$UA" "<url>" | grep -iE '^(HTTP/|server|cf-ray|cf-mitigated|content-type)'
```

要抓的页面:**首页**(找搜索表单 + 站点结构)、**一本书的详情页**、它的**目录页**、一个**搜索结果页**、一个**分类页**。常用观察手段:

```bash
grep -oiE '<form[^>]*>|name="[a-z_]+"|action="[^"]*"' /tmp/home.html | head   # 搜索表单
grep -oiE 'property="og:[^"]*"|content="[^"]*"' /tmp/book.html | head          # og:meta
grep -oiE 'class="[^"]*"' /tmp/page.html | sort | uniq -c | sort -rn | head    # 高频 class
sed -e 's/<[^>]*>/ /g' /tmp/page.html | tr -s ' \n' ' \n' | grep -v '^ *$' | head  # 去标签看文本
```

## 按能力逆向

### bookInfo(书详情)
优先 `og:` meta,稳定且跨站一致:
```json
"bookInfo": {
  "name":    { "via": "css", "select": "[property=\"og:novel:book_name\"]", "extract": { "attr": "content" } },
  "author":  { "via": "css", "select": "[property=\"og:novel:author\"]",    "extract": { "attr": "content" } },
  "cover":   { "via": "css", "select": "[property=\"og:image\"]",            "extract": { "attr": "content" } },
  "intro":   { "via": "css", "select": "[property=\"og:description\"]",      "extract": { "attr": "content" } },
  "tocUrl":  { "via": "css", "select": "[property=\"og:novel:read_url\"]",   "extract": { "attr": "content" } }
}
```
没有 og:meta 时退到页面元素(`h1`、`.book-name`、`.author` 等),用 `firstOf` 兜底。

### toc(目录 + 分卷)
难点是**卷与章在同一列表里、要保序切分**。让 `list` 同时选中卷标题与章节链接,`isVolume` 区分:
```json
"toc": {
  "list":     { "via": "css", "select": ".box > h2.module-title.type, .box a.module-row-text" },
  "name":     { "firstOf": [ { "via":"css","select":".module-row-title","extract":"text" },
                             { "via":"css","select":"h2","extract":"text" } ] },
  "url":      { "via": "css", "select": "a", "extract": { "attr": "href" } },
  "isVolume": { "via": "css", "select": "h2", "extract": "text" },
  "maxPages": 1
}
```
`isVolume` 对卷节点求值非空、对章节为空 → 引擎据此切「卷→章」。目录翻页:加 `nextPage`(选下一页链接的规则)+ `maxPages`。无分卷的站把 `isVolume` 省略即可(全部当章节)。

**「最新章节预览」去重陷阱(高频)**:很多目录页开头有一块「最新章节」(约 12 条,**倒序、且与正文章节区重复**)。直接选所有章节链接会得到重复+乱序。用**兄弟选择器**只取「正文」标记之后的:如 `#list dl dt:has(a[href*="txt_"]) ~ dd a`(选正文 `<dt>` 之后的 `<dd> a`),或定位正文专属容器。选完核对章节数与正序。

### content(正文)
```json
"content": { "value": { "via": "css", "select": ".article-content", "extract": "html" } }
```
`extract:"html"` 把 `<p>`/`<br>` 转成换行再清理标签。正文分页:`"nextPage": {…}, "maxPages": 20`。注意排除「上一章/下一章/广告」节点(用更精确的 select 或 `clean` 正则删页脚)。

### 字体反爬(自定义字体 + PUA)

**现象**:正文/书名夹大量私有区字符(`U+E000–U+F8FF`,常显示为豆腐 □),页面靠 `@font-face` 自定义字体渲染回真字。你抓到的是 PUA 码点,换环境就是乱码。

**为什么不能直接读字体翻译**:加密字体的 cmap 只把 PUA 映到 `gidNNNNN`(母字库内部编号),不告诉你是哪个字。只能靠**字形长相**——渲染 PUA 字形,和标准中文字体逐个常用字比像素,最像的即真字。

**还原流程**:

1. 从页面 CSS 取加密字体 URL(`@font-face` 的 `src:url(….woff2)`)。
2. 跑生成命令:
   ```bash
   trn gen-fontmap <字体URL或本地woff2> -o fontmap.json [-b 基准字体]
   ```
   纯 Rust:`woff2-patched` 解压 + `swash`(skrifa+zeno)渲染字形 + GB2312 一级 3755 候选 + 余弦相似度匹配。基准字体缺省自动下载 Noto;输出 `{码点:真字}` JSON,并把低置信(可能形近认错)的字单独列出。
3. 把表内联进正文 clean(先用 `extract` 取到含 PUA 的文本,再还原):
   ```json
   "content": { "value": { "via":"css", "select":".content", "extract":"html",
     "clean": [ { "fontMap": { "E4DE":"一", "E4F3":"的", "E41E":"是" } } ] } }
   ```

**要点**:
- 同一站通常**全站一套字体** → 映射固定,生成一次即可复用;站方换字体(woff2 URL 变)才需重跑。
- 引擎只做 O(1) 查表替换,**不内置任何站点的表**——表是数据,内联进书源。
- 命令列出的低置信字(形近,如「已/己」「土/士」)值得人工瞄一眼校正。
- 若正文藏在 `<script>` 的 JSON(如 `window.__INITIAL_STATE__`)而非可见 DOM,先用 `via:"regex"` 抽出 JSON 串、再 `via:"json"` 取正文字段,然后 fontMap 还原。
- 完整原理(从零)与命令实现见博客 `dev-notes/blog/font-anti-scraping-and-fontmap.md`。

### search(搜索)
从首页表单拿 action 与参数名;`item` **必须含 `bookUrl`**:
```json
"search": {
  "request": { "url": { "template": "{{base}}/search.html?searchkey={{key}}" }, "method": "GET" },
  "list": { "via": "css", "select": ".module-search-item" },
  "item": {
    "bookUrl": { "via": "css", "select": "h3 a", "extract": { "attr": "href" } },
    "name":    { "via": "css", "select": "h3 a", "extract": "text" },
    "cover":   { "via": "css", "select": ".module-item-pic img", "extract": { "attr": "data-src" } }
  }
}
```
**坑**:
- 搜索结果页常把「真实结果」和「热门推荐位」混排,且可能用**不同 class**。务必定位结果专属容器(如 `.module-search-item`),别误选推荐位。
- POST 搜索:`"method":"POST","body":{"template":"searchkey={{key}}&searchtype=all"}`;不少站还**必须显式带表单头**才返回结果——`"headers":{"Content-Type":"application/x-www-form-urlencoded"}`。
- 搜索接口是 JSON 时用 `via:"json"` 解(如 `$[*]` / `$.url_list`);若接口 token 门控,见下「反爬」的 warmup 套路。
- 选好后**换两个不同关键词验真**:确认结果随词变化,而非固定榜单。

### explore(浏览)
```json
"explore": {
  "categories": [ { "title": "玄幻", "url": { "template": "{{base}}/sort/1_{{page}}.html" } } ],
  "list": { "via": "css", "select": ".module-item" },
  "item": { "bookUrl": {…}, "name": {…} }
}
```
浏览是搜索被反爬时的降级入口,优先做稳。

## 字符集
中文乱码 → `http.charset`: `"gbk"`(最常见)/`"gb18030"`/`"big5"`;默认 `"auto"`(UTF-8 失败回退 GBK)。判断:`curl ... | file -` 或看 `<meta charset>`,或直接 doctor 看书名/正文是否乱码。

## 反爬
检测:响应头 `cf-mitigated: challenge`,或 403/503 且 body 含 `_cf_chl_opt`、`/cdn-cgi/challenge-platform/`、`<title>Just a moment`。

处置顺序:
1. **找未被挑战的等价入口**。很多站只锁伪静态路径(`/search.html`),底层路由(`/index.php/vod/search.html?wd=…`)或 `/index.php/ajax/suggest` 未被挡 → 直接换 URL,零反爬。**必须验真**:换两个不同关键词对比结果,确认它真按词过滤,而非「不管查啥都回同一榜单」的假入口(后者不可用)。
2. **标取页模式 `http.fetcher`**:仅搜索被挑战、整体可读 → `"auto"`(默认,撞挑战才用系统浏览器 headful 解 `cf_clearance` 再交回快请求);首请求即被挑战/正文 JS 渲染 → `"browser"`;永不开浏览器 → `"reqwest"`。
3. **判「不可行」前先试低成本绕过**(很多拦截很浅):
   - 章节页返回「加载中…」的 JS 跳转桩(HTTP 200、**非** CF 挑战,所以 `fetcher:browser` 救不了):试**在章节 URL 后加无害 query**(如 `?nocache=1`)——常直接吐出带 `#chaptercontent` 的真正文。落地:在 toc 的 `url` 规则上 `clean.append: "?nocache=1"`。
   - 正文/搜索是 **token 门控的 JSON 接口**(需先有某 cookie):把下发该 cookie 的辅助端点(如 `https://站点/user/hm.html`)加进 `http.warmup`,引擎的 cookie_store 会跨请求保留,再用 `via:"json"` 解接口。
4. **接受降级**。挑战自适应,有时需用户在弹出浏览器里点一下 Turnstile;阅读链路通常开放,搜索 ✗ 不影响读书,引导用浏览即可。

**架构边界(诚实告知用户)**:若**整页正文由站点 JS 异步渲染 / 加密接口动态拉取**(如 SPA + CryptoJS 签名的 `/api/*`,无任何静态路径返回正文文本),当前 reqwest + cookie 烤箱**无法读取**;`fetcher:browser` 只是 CF 挑战求解器、**不会跑站点的内容加载 JS**。这类站只能做到浏览/书详情/目录,正文需要「能执行页面 JS 的渲染型 fetcher」(暂不支持)。把能做的做好、保留正文选择器兜底,并如实说明。

## 常见 CMS 模式
- **MXCMS / 苹果CMS(MacCMS)系**:静态目录 `/mxstatic/`、class 多为 `module-*`、底层路由 `/index.php/vod/...`、联想接口 `/index.php/ajax/suggest?mid=&wd=`;书详情常有 `og:novel:*` meta。`example-bilixs.v2.json` 即此类。
- 章节页 URL 常形如 `/novel/<id>/<chap>.html`,目录 `/novel/<id>/catalog`。
- 封面图常用懒加载:真实地址在 `data-src`(不是 `src`)。

## doctor 迭代
```bash
cargo build        # 一次
./target/debug/trn doctor <文件>   # 反复
```
读法:`✓` 通过 / `✗` 异常(看 detail)/ `○` 跳过(未配置或缺前置)。策略:
- 先让 `配置` ✓(JSON 能解析)、再让 `浏览` 或 `samples` ✓(解锁书详情/目录/正文的探测)。
- 一次只改一条规则,重跑,逐项转绿。
- `目录` 的 detail 形如「N 卷 / M 章」,据此核对分卷是否正确。
- 搜索 `✗` 且 detail 说「被反爬挑战」属预期(见反爬),非规则 bug。
