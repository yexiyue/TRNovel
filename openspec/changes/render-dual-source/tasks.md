## 0. 前置 spike(真浏览器)—— 已完成(agent-browser 实测 2026-06-11)

- [x] 0.1 番茄 explore 书库分页器 = 字节 Byte 设计系统 `byte-pagination` 组件。结构:`<div.byte-pagination><ul.byte-pagination-list><li.byte-pagination-item>…`,项序为 `[prev-icon, 1,2,3,4,5, …jumper, 99, next-icon]`。**总页数 = 最后一个数字项**。选择器:`.byte-pagination-item:not(.byte-pagination-item-icon):not(.byte-pagination-item-jumper)` 取 `index:-1` → 文本 `"99"`(纯数字、无需 regex,但加数字守卫稳妥;少页/无 jumper/单页均成立——末尾恒是 next-icon,过滤后末项即总数)
- [x] 0.2 `search`(`/search/{词}`)**同样**有 `byte-pagination`(同组件、同选择器),「十日终焉」实测 30 页(10 条/页);故 **explore + search 一套选择器通吃**,本 change 覆盖二者(不再只 explore)。注:总页数在首页 DOM 即可读,与 search「点击驱动翻页」机制无关

## 1. 双源 render 基建

- [ ] 1.1 `fetch/mod.rs`:`FetchResponse` 增 `dom_html: Option<String>`(注释界定仅 render+intercept 路径有值)
- [ ] 1.2 `fetch/browser/fetcher.rs`:`render_intercept` 拦到 API body 后,**当调用方要求 DOM**(传入「需 DOM」标志/就绪选择器)时,等 `ready_for`(复用 `render_dom_page` 的 MutationObserver 等待)、抓 `outerHTML`,返回 `(api_body, Option<dom>)`;关 Page 留 Browser 不变
- [ ] 1.3 `FetchRequest` 增「DOM 就绪选择器 / 需 DOM」表达(放宽 `intercept_api`+`ready_for` 共存);`EscalatingFetcher` 透传 `dom_html`
- [ ] 1.4 browser 模块单测:配 DOM 源 → `dom_html` 为 `Some`;未配 → `None`(行为不变)

## 2. 配置与求值路由

- [ ] 2.1 `source/{op.rs,http.rs}`:`ExploreOp`/`SearchOp`/`Request` 增 `total_pages: Option<Rule>`(向后兼容,默认无);放宽 `intercept_api`+`ready_for` 校验为可共存
- [ ] 2.2 `engine`:求值上下文携带「API body + 可选 DOM 源」;规则按 `Rule.via` 路由(`json`→API、`css`/`xpath`→DOM)。仅当 op 含 DOM 源规则时,取页才请求 DOM(D5)
- [ ] 2.3 `engine`:`explore`/`search` 返回 `BookList` 增 `total_pages`;单页取页后对 `totalPages` 规则求值一次、解析 `u32`(失败/空→`None`)
- [ ] 2.4 schema 重生成 + `source`/`engine` 测试(解析 / round-trip / `deny_unknown_fields` / via 路由 / 向后兼容)

## 3. UI

- [ ] 3.1 `find_book`:底部页码「第 N 页」→「第 N / M 页」(有 `total_pages` 时);`None` 退化为现状
- [ ] 3.2 其余调用方 / doctor 适配 `BookList.total_pages`

## 4. 番茄接入与验证

- [ ] 4.1 `fanqie-web.v2.json`:explore **与 search** 都加 `ready_for:".byte-pagination"`(分页器就绪闸)+ `totalPages:{via:css, select:".byte-pagination-item:not(.byte-pagination-item-icon):not(.byte-pagination-item-jumper)", index:-1, clean:[regex 抽数字]}`(0.1/0.2 实测同组件通用)
- [ ] 4.2 `cargo build` 主程序(验 `Send` 不回归)+ `test --all-features` + `clippy -D warnings` + `doc` + `fmt`,全绿
- [ ] 4.3 UI 实跑:番茄书库 explore 显示「第 N / 99 页」,翻页准确(待用户浏览器环境)

## 5. 文档

- [ ] 5.1 改正 `list-has-more` design D3(「无 DOM」前提作废,指向本 change)
- [ ] 5.2 知识库 `dev-notes/knowledge/booksource.md`:render 双源(API JSON + DOM,`via` 路由)+ 分页器选择器坑
