## Context

render 取页有两种方式(`render-fetcher`):`ready_for`(取渲染后 DOM)与 `interceptApi`(拦签名 API 响应体)。番茄 `explore`/`search` 用 `interceptApi`:`intercept_page` 驱动真浏览器导航、跑 JS,拦 `book_list/v0`/`search_book/v1` 的 JSON 当数据源,返回**一个** `body`(API JSON)。

数据流现状:`FetchResponse { body, status, headers }` 单 body → 引擎对 `body` 按规则(`list`/`item`,以及 `list-has-more` 的 `has_more`,均 `via:json`)求值。

`list-has-more` D3 拒绝精确总页数,理由「explore 拦 API 无 DOM」——**该前提错误**:`intercept_page` 用的是活页面,DOM 一直在,分页器(番茄实测 99 页)就在 DOM 里,只是 `render_intercept` 抓完 API body 就 `page.close()`,把 DOM 丢了。`total_count` API 字段实测恒为 10000 占位,不可用;真总数只在 DOM 分页器。

## Goals / Non-Goals

**Goals:**
- render(intercept)把「拦到的 API JSON」+「渲染后的 DOM」暴露成两个 `via` 可寻址的源(**通用能力**,非总页数特判)。
- `explore`/`search` 增通用 `totalPages: Option<Rule>`,返回 `BookList.total_pages: Option<u32>`,UI 显示「第 N / M 页」。
- 向后兼容:无 `totalPages` = 不抓 DOM、不返回总数 = 现状。

**Non-Goals:**
- 不为非分页数据再设别的源(本 change 只暴露「拦截 API JSON + 渲染 DOM」两源;够覆盖 totalPages 与一般 DOM 字段)。
- 不并发渲染(沿用 `browser-pool` 的 `BROWSER_LOCK` 串行)。

> spike 已确认(agent-browser 实测):番茄 explore 书库**与** search 用**同一个**字节 `byte-pagination` 组件,总页数 = 分页器最后一个数字项,选择器 `.byte-pagination-item:not(.byte-pagination-item-icon):not(.byte-pagination-item-jumper)` + `index:-1`(库 99 页、search「十日终焉」30 页均验证)。故 explore + search **一套通吃**,本 change 覆盖二者。

## Decisions

**D1 — render(intercept)双源:API JSON + 可选渲染 DOM,规则按 `via` 路由。**
- `render_intercept` 拦到 API body 后,**当本 op 配了「DOM 源规则」**(`via:css`/`xpath` 的 `totalPages` 等)时,等就绪选择器出现、抓 `document.documentElement.outerHTML`,与 API body 一并返回。
- 引擎求值时按 `Rule.via` 路由:`via:json` → API body;`via:css`/`xpath` → DOM。`list`/`item`/`has_more`(番茄均 `via:json`)不变,仍打 API body;`totalPages`(`via:css`)打 DOM。
- **备选(否决)B —— render 层直接 `page.evaluate` 选择器只回一个值**:把规则求值劈到浏览器层,破坏「规则即数据、统一在 `eval` 引擎求值」的分层,且丢失 chain/fallback/regex/clean 全套规则能力。故选「render 只负责把 DOM 这一份原样带出,求值仍在引擎」。

**D2 — `totalPages` 是通用书源规则(规则即数据,同 `has_more` 的 D1)。**
- `ExploreOp`/`SearchOp`/`Request` 增 `total_pages: Option<Rule>`;引擎单页取页后对**该规则 `via` 指向的源**求值一次,解析为 `u32`(规则可含 regex 抽数字 + clean);失败/空 → `None`。
- **两档,成本天差**:`via:json`(API 总数靠谱的源)/ 非 render 书源 `via:css`(reqwest HTML)**零基建**——本就只有一个 body,via 路由到它即可;只有「render + 总数在 DOM」(fanqie)才用到 D1 的双源基建。即同一个通用规则,便宜档白送,DOM 档靠基建。

**D3 — 时序:抓 DOM 前等就绪闸;放宽 `intercept_api` + `ready_for` 可共存。**
- 分页器在 API 响应之后由 JS 渲染,拦到 API body 的瞬间分页器可能尚未入 DOM。故抓 `outerHTML` 前,若配了 `ready_for`,先等该选择器出现(有界,复用 `render_dom` 的 MutationObserver 等待);未配则抓当前 DOM(尽力)。
- 现状 `intercept_api` 与 `ready_for` 是「二选一」;本 change 放宽为**可同时给**:`intercept_api` 取数据、`ready_for` 作 DOM 就绪闸。

**D4 — 传输:`FetchResponse` 增 `dom_html: Option<String>`。**
- 最小改动:render+intercept 抓到 DOM 时置 `Some`,其它路径 `None`;`EscalatingFetcher` 透传,引擎据此构建「API body + 可选 DOM」的双源求值上下文。
- 备选(否决):另立 `RenderResult` 类型走独立通道 —— 渲染本就经 `Fetcher::fetch_full → FetchResponse` 统一管线,新通道要在装饰器链各层多穿一份,得不偿失。代价:`FetchResponse`(通用 fetch 抽象)渗入一个 render 概念字段,以注释界定其仅 render+intercept 路径有值。

**D5 — 仅在需要时抓 DOM。**
- 只有本 op 存在「DOM 源规则」(`via:css`/`xpath` 的 `totalPages`)时,`render_intercept` 才等闸 + 抓 `outerHTML`;否则一律不抓(零额外开销、不 ship 整页 HTML),与现状逐字节一致。

**D6 — 返回 + UI。**
- `explore`/`search` 返回 `BookList { items, has_more, total_pages: Option<u32> }`(`total_pages` 在 `list-has-more` 的 `BookList` 上 additive)。
- `find_book`:有 `total_pages` 时底部显示「第 N / M 页」;`None` 退化为「第 N 页」(现状)。配合 `has_more` 的到头停翻,M 还能给「下一页」键一个更准的边界(可选)。

**D7 — 与 `list-has-more` 的边界。**
- 本 change **supersedes** `list-has-more` D3(其「无 DOM」前提作废);`list-has-more` 仍独立交付 `has_more`(布尔边界),本 change 只在其 `BookList` 上加 `total_pages` 字段并新增双源能力。两者共享返回类型,先落地者建 `BookList`,后者加字段。

## Risks / Trade-offs

- [`FetchResponse` 渗入 render 概念 `dom_html`] → 注释界定仅 render+intercept 有值;D4 否决了更重的独立通道。
- [DOM 体积:为读一个数字 ship 整页 HTML] → D5 仅在配了 DOM 源规则时才抓;可后续优化为「只抓分页器子树」。
- [分页器渲染时序] → D3 就绪闸(`ready_for`)等待。
- ~~[fanqie 分页器选择器未知]~~ → **已 spike 解决**:`byte-pagination` 组件,末数字项即总数(见 Goals 下的实测注)。
- ~~[`search_book/v1` 是否有总数分页器未确认]~~ → **已 spike 解决**:search 同组件、同选择器,explore + search 通吃。
- [改 `explore`/`search` 返回类型 + 引擎双源路由] → 牵动 `find_book`/doctor/engine 测试;与 `list-has-more` 协调 `BookList`。
- [`Send` 回归] → render Future 仍在 `tokio::spawn` 内,改完必 `cargo build` 主程序验证。
