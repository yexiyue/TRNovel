---
name: booksource-generator
description: >-
  从一个小说网站 URL 生成 TRNovel 书源(trnovel-booksource/v2 JSON)。当用户给出小说/阅读类网站并希望接入
  TRNovel、做书源、适配书源、逆向某站点目录/正文时使用——例如「给这个站做个书源 https://…」「generate a
  book source for X」「把这个小说网站接入 trnovel」「帮我解析这个站的章节/搜索」,即便没明说「书源」二字。
  本 skill 会探测真实站点、逆向 search/浏览/书详情/目录(含分卷)/正文 的 CSS/JSONPath 选择器,处理字符集与
  Cloudflare 反爬,写出 JSON,并用 `trn doctor` 全流程校验、按 ✓/✗ 迭代到通过。
---

# 书源生成器(TRNovel v2)

## 这是什么 · 书源的本质

书源是一份**声明式 JSON**,教 TRNovel 如何读一个小说站:去哪搜索、怎么浏览分类、从书详情页抽取书名/作者/封面、列出目录(章节 + 分卷)、抓取每章正文。你的任务:**给定站点 URL,逆向它的 HTML,产出一份能通过 `trn doctor` 的 `trnovel-booksource/v2` JSON。**

格式是**结构化**的(没有紧凑字符串 DSL):每个字段是一条「规则」——一次 CSS/JSONPath/正则抽取,或组合子(firstOf/concat/literal/template)。

- 精确字段与规则语法 → 读 `references/book-source.schema.json`
- 一份完整可运行的真实示例 → 读 `references/example-bilixs.v2.json`
- 逐能力的探站/逆向战术、curl 探测配方、字符集与反爬细节 → 读 `references/reverse-engineering.md`

边做边查这三个文件,不要凭记忆硬编规则语法。

## 核心循环(照做)

1. **探站**:抓取代表性页面——首页、一本书的**详情页**、它的**目录页**、一个**搜索结果页**、一个**分类浏览页**。用 curl/WebFetch,保存 HTML,观察结构。
2. **逐能力逆向选择器**:search / explore / bookInfo / toc / content(见下「按能力逆向」)。
3. **判定字符集 + 反爬**(见下两节)。
4. **拼装 JSON**(`schema: "trnovel-booksource/v2"`,带 `samples` 黄金样例)。
5. **校验:`trn doctor <文件>`** → 看逐项 ✓/✗ → 只修那个 ✗ 对应的规则 → 重跑,直到全绿(或只剩已知难项,如被挑战的搜索)。
6. 存为 `<站点>.v2.json`,交给用户。

**绝不把未经 doctor 校验的书源交付。** 这个校验循环就是整件事的核心——它把「猜的选择器」变成「验证过的选择器」。

## 用 doctor 做反馈引擎

在仓库根目录:

```bash
cargo run -- doctor /path/to/<站点>.v2.json
# 或已安装的二进制:trn doctor <文件>
```

它跑完整流程并对 **配置 / 浏览 / 书详情 / 目录 / 正文 / 搜索** 打 ✓/✗/○。读每个 ✗ 的 detail,**一次只修一条规则**,再跑。无样例时它会用浏览结果探一本书来测读取链路,所以**先把 `explore` 或 `samples` 做对**能解锁后面几项。

迭代提速:可先 `cargo build` 一次,然后反复跑 `./target/debug/trn doctor <文件>`(见 `scripts/validate.sh`)。

## 按能力逆向(关键启发式)

每条规则的 `select` 是 **self-or-descendant** 语义;HTML 默认 `via: "css"`。下面是从真实站点(MXCMS/MacCMS 系等)总结的高命中套路,细节见 `references/reverse-engineering.md`。

- **bookInfo(书详情)**:**先找 `og:` meta**——`<meta property="og:novel:book_name">`、`og:novel:author`、`og:novel:read_url`(常是 tocUrl)、`og:image`(封面)、`og:description`。用 `{"via":"css","select":"[property=\"og:novel:book_name\"]","extract":{"attr":"content"}}`。比靠页面可见元素稳得多。
- **toc(目录 + 分卷)**:目录页里**卷标题**(如 `第一卷`)和**章节链接**通常是兄弟节点。用一个 `list` 选择器**同时选中两者并保持文档顺序**(如 `.box > h2.module-title, .box a.module-row-text`);再用 `isVolume` 规则判定是否卷(对卷节点非空、对章节为空,例如选 `h2`);`name`/`url` 用 `firstOf` 兼容两种节点。引擎据 `isVolume` 切分「卷→章」。**常见坑**:目录开头常有 N 条「最新章节」预览(倒序、与正文区重复),用兄弟选择器(如 `#list dl dt:has(a[href*="txt_"]) ~ dd a`)只取「正文」标记之后的章节,避免重复/倒序。
- **content(正文)**:找正文容器(`.article-content`/`#content`/`.read-content` 等),`extract: "html"` 会把标签转成换行后清理;分页正文用 `nextPage` + `maxPages`。
- **search(搜索)**:从首页 `<form>` 找 action 与 input name(如 `searchkey`/`wd`)。`request.url` 用模板 `{{base}}/search.html?searchkey={{key}}`;`list` 选结果条目容器(**注意区分真正的结果区与推荐位**——它们可能用不同 class);`item` 里**必须含 `bookUrl`**(指向书详情)。POST 搜索用 `method:"POST"` + `body` 模板。
- **explore(浏览)**:`categories` 列分类(title + url 模板,常含 `{{page}}`);`list`/`item` 同 search。浏览是搜索被反爬挡住时的**降级入口**,尽量做好。

`{{base}}` = 站点根、`{{key}}` = 搜索词、`{{page}}`/`{{pageSize}}` = 分页。相对 URL 引擎会自动补全,**抽取 href 时保留相对路径即可**。

## 字符集

老站常是 GBK。若抓下的中文乱码,设 `http.charset: "gbk"`(或 `gb18030`)。不确定就用默认 `auto`(UTF-8 失败回退 GBK),doctor 正文/书名出现乱码即调。

## 反爬(Cloudflare 等)

若某端点(常是**搜索**)返回挑战页,doctor 会报「被反爬挑战拦截,需浏览器辅助或改用浏览」。处理顺序:

1. **先找未被挑战的等价入口**:很多站只锁伪静态搜索路径,底层路由(如 `/index.php/vod/search.html?...`)或 JSON 接口未被挡。换上去就不用反爬。**但要核实它真在按关键词过滤**(换两个词对比结果),别被「不管查啥都返回同一榜单」的假入口骗了。
2. **绕不开就标注取页模式**:站点整体能读、仅搜索被挑战 → `http.fetcher: "auto"`(默认,平时 reqwest、撞挑战才用系统浏览器解 `cf_clearance`);整站首请求即被挑战 / 正文靠 JS 渲染 → `"browser"`;想永不开浏览器 → `"reqwest"`(撞挑战即降级)。
3. **判「不可行」前先试低成本绕过**(很多站的拦截是浅的):
   - 章节页返回「加载中…」的 JS 跳转桩(HTTP 200,**不是** CF 挑战,`fetcher:browser` 救不了)?试**在 URL 后加个无害 query**(如 `?nocache=1`,用 `clean.append`)常能直接绕到真正文。
   - 正文/搜索是 **token 门控的 JSON 接口**?把发 token 的辅助端点(如 `/user/hm.html`)加进 `http.warmup` 拿 cookie,再用 `via:"json"` 解接口(cookie_store 会跨请求保留)。
4. **接受降级**:阅读链路通常开放,搜索被挑战不影响读书。doctor 把搜索标 ✗ 但 detail 精确,即视为「需浏览器/改用浏览」,不是规则 bug。

**架构边界**:若**整页正文由站点自身 JS 异步渲染 / 加密接口动态拉取**(没有任何静态路径返回正文文本),当前 reqwest + cookie 烤箱**拿不到**,`fetcher:browser` 也只解 CF 挑战、不跑站点内容 JS。此时如实告知用户「该站正文需能执行页面 JS 的渲染型取页(暂不支持)」,把能做的(浏览/书详情/目录)做好并保留正文选择器兜底。

详见 `references/reverse-engineering.md` 的「反爬」一节。

## 拿不准就问(用 AskUserQuestion)

探站时遇到**你无法从页面自行判定**的事,别瞎猜——用 AskUserQuestion 问用户。典型:

- 站点需要登录/会员才能看正文或目录;
- 有多个候选区块,分不清哪个是「正文/书单」主体;
- 没有可用的样例书(`samples` 需要一个真实 book_url + 期望);
- 站点疑似需要浏览器才能拿到内容,问是否启用 `fetcher: "browser"`;
- 同一意图有多个搜索入口,问优先哪个。

把可推断的(默认 charset、相对 URL、auto fetcher)直接定下来,只把**真正需要用户拍板**的拿出来问。

## 产物

- `<站点>.v2.json`:`schema`/`name`/`url`/`http?`/`search?`/`explore?`/`bookInfo`/`toc`/`content`/`samples`。
- 至少一条 `samples`(一个真实 `bookUrl` + `expect`,如 `name`、`minChapters`、`volumes`、`minContentChars`)——既驱动 doctor 在无浏览时也能验,也供运行期自愈。
- 一句话交付小结:doctor 各项结果、哪些 ✓、哪些是已知降级项(如被挑战的搜索)。
