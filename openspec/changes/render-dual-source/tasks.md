## 0. 前置 spike(真浏览器)—— 已完成(agent-browser 实测 2026-06-11)

- [x] 0.1 番茄 explore 书库分页器 = 字节 Byte 设计系统 `byte-pagination` 组件。结构:`<div.byte-pagination><ul.byte-pagination-list><li.byte-pagination-item>…`,项序为 `[prev-icon, 1,2,3,4,5, …jumper, 99, next-icon]`。**总页数 = 最后一个数字项**。选择器:`.byte-pagination-item:not(.byte-pagination-item-icon):not(.byte-pagination-item-jumper)` 取 `index:-1` → 文本 `"99"`(纯数字、无需 regex,但加数字守卫稳妥;少页/无 jumper/单页均成立——末尾恒是 next-icon,过滤后末项即总数)
- [x] 0.2 `search`(`/search/{词}`)**同样**有 `byte-pagination`(同组件、同选择器),「十日终焉」实测 30 页(10 条/页);故 **explore + search 一套选择器通吃**,本 change 覆盖二者(不再只 explore)。注:总页数在首页 DOM 即可读,与 search「点击驱动翻页」机制无关

## 1. 双源 render 基建

- [x] 1.1 `fetch/mod.rs`:`FetchResponse` 增 `dom_html: Option<String>`(注释界定仅 render+intercept 路径有值)
- [x] 1.2 `fetch/browser/fetcher.rs`:`render_intercept` 增 `dom_ready: Option<&str>`,拦到 API body 后(有 `dom_ready` 时)等就绪(抽 `wait_ready`/`outer_html`,与 `render_dom_page` 共用)、抓 `outerHTML`,返回 `(api_body, Option<dom>)`;关 Page 留 Browser 不变
- [x] 1.3 信号 = `intercept_api` + `ready_for` 共存(无新字段);`FetchRequest`/`Request` 文档放宽「二选一」;`EscalatingFetcher` 把 `ready_for` 作 `dom_ready` 传入、透传 `dom_html`
- [x] 1.4 dom-presence 路由经引擎测试(假 `DualSourceFetcher` 回 `dom_html`)覆盖,见 2.4

## 2. 配置与求值路由

- [x] 2.1 `source/{op.rs,http.rs}`:`ExploreOp` + `Request`(覆盖 search)增 `total_pages: Option<Rule>`(向后兼容,默认无);`intercept_api`+`ready_for` 无硬校验、文档放宽为可共存
- [x] 2.2 `engine`:**实现取舍——dom-presence 路由**而非逐规则内省 `Rule.via`(Rule 是枚举、组合规则可混 via,内省太重)。`eval_total_pages`:抓到 DOM(`dom_html`)→ 对 DOM 求值 `totalPages`,否则对 body;`list`/`item` 恒对 body。作者配 `ready_for`(→抓 DOM)正是 css-totalPages 要 DOM 的 opt-in,故等价满足 spec 的 via 路由意图(见 design D1 补注)。仅 op 含 DOM 源(配 `ready_for`)时才抓 DOM(D5)
- [x] 2.3 `engine`:`explore`/`search` 返回 `BookList { items, total_pages }`;单页取页后对 `totalPages` 求值一次、`parse_total_pages` 抽首段数字为 `u32`(失败/空→`None`)
- [x] 2.4 schema 重生成(`gen_schema`)+ `schema_is_in_sync` 绿;新增 `total_pages_from_dom_via_css` / `_from_body_when_no_dom` / `_none_without_rule` 三测(via 路由 / 便宜档 / 向后兼容)

## 3. UI

- [x] 3.1 `find_book`:底部页码「第 N 页」→「第 N / M 页」(有 `total_pages` 时);`None` 退化为现状
- [x] 3.2 其余调用方适配 `BookList.items`/`.total_pages`:`verify.rs`(doctor 诊断,顺带显示「共 M 页」)、`engine_search_poc`/`spa_search_poc` 示例

## 4. 番茄接入与验证

- [x] 4.1 `fanqie-web.v2.json`:explore **与** search.request 都加 `readyFor:".byte-pagination"` + `totalPages:{via:css, select:".byte-pagination-item:not(.byte-pagination-item-icon):not(.byte-pagination-item-jumper)", index:-1}`(`index:-1` 取末数字项,选择器已过滤 icon/jumper,无需 regex);`from_json` 解析验证两处 totalPages 均在
- [x] 4.2 `cargo build` 主程序(`Send` 不回归)+ `test --all-features`(130 passed)+ `clippy -D warnings` + `doc -D warnings` + `fmt`,全绿
- [ ] 4.3 UI 实跑:番茄书库 explore 显示「第 N / 99 页」、search「第 N / 30 页」,翻页准确(**待用户浏览器环境**——沙箱无 TUI/真渲染交互)

## 5. 文档

- [x] 5.1 改正 `list-has-more` design D3(「无 DOM」前提作废,指向本 change)——已于捕获阶段完成
- [x] 5.2 知识库 `dev-notes/knowledge/booksource.md`:render 双源(API JSON + DOM,dom-presence 路由)+ 番茄 `byte-pagination` 选择器
