## Why

`render-fetcher` 把番茄网页**搜索第一页**接通了,但 `explore`(浏览)还完全取不到 SPA 数据:`render`/`readyFor`/`interceptApi` 只挂在 `Request` 上,而 `explore` 用的是 `Category`(只有 `title`/`url`),`engine.explore` 走纯 reqwest `fetch_checked` —— 番茄书库是 SPA,reqwest 拿到的是空壳(design `render-fetcher` L39 已记此为 future 口子)。

**翻页不是引擎缺能力**:UI(`find_book.rs`)用「单页 + 用户按键递增 `page`」翻页,`explore(category, page)` 只需取第 `page` 页(`{{page}}` 映射 URL)。番茄书库之前翻不了,**根因是 explore 取不到 SPA 数据**,而非缺分页机制。真浏览器 spike 实测:番茄书库 `/library/all/page_N` 是 URL 驱动,直接导航即渲染第 N 页(拦 `book_list/v0?page_index=N-1`)——所以只要 explore 能 render,单页 + UI 递增 `page` 就能翻页。

## What Changes

- **给 `explore` 开渲染通道**:`explore` 的分类请求对齐 `search` 主请求,支持 `render`/`readyFor`/`interceptApi`(`ExploreOp` 内联,分类间共享),使 SPA 站点的浏览/分类列表可取(番茄书库不再空壳)。
- **`explore`/`search` 保持单页**:`{{page}}` 映射 URL,只取调用方传入的第 `page` 页;**交互翻页由 UI 递增 `page` 驱动**,引擎不做批量翻页。
- **番茄书库接入**:`fanqie-web.v2.json` 的 `explore` 改为 `render:true` + `interceptApi:book_list/v0`、分类 URL 模板 `{{base}}/library/all/page_{{page}}?sort=hottest|newest`、`list`/`item` 用 `via:json`,书库浏览翻页可用(一次只开一个浏览器)。
- **向后兼容**:`explore` 不开 `render` = 现状 reqwest 直取;默认零行为变化。全部仅在已有 feature 门控下编译。

> **注**:曾尝试在引擎做 `by:url` 批量翻页(`nextPage` 配置),实测与 UI 的「单页 + 递增 page」模型冲突(一进书库连开 N 个浏览器、内容重复),在 TRNovel 无正确用例,已回退。**动态总页数**(UI 显示「共 M 页」/边界)、番茄 **search 翻页**(URL 不认页码,需点击驱动)、**浏览器池化**均为正确的后续方向,留独立 change(详见 design Non-Goals / Open Questions)。

## Capabilities

### New Capabilities
- `render-list-pagination`: `explore` 的渲染取页通道(对齐 `search` 的 `render`/`readyFor`/`interceptApi`),使 SPA 浏览/分类列表可取;`explore`/`search` 单页取页 + `{{page}}` 映射 URL,交互翻页由 UI 递增 `page` 驱动。

### Modified Capabilities
<!-- render-fetcher 尚未归档进 openspec/specs/(仍为 active change),其"render 仅 search 主请求"的边界由本 change 的新 capability 扩展,不构成对已归档 spec 的需求变更。 -->

## Impact

- `crates/parse-book-source/src/source/op.rs`：`ExploreOp` 内联 `render`/`readyFor`/`interceptApi`(`Category` 保持 `{title,url}`,url 模板带 `{{page}}`);向后兼容、默认无。
- `crates/parse-book-source/src/engine/mod.rs` + `engine/internal.rs`：`engine.explore` 取页按 `op.render` 走渲染/CDP 拦截或 `fetch_checked`,与 `search` 同款路由;`explore`/`search` 均为**单页**普通 `async fn`(无 async 闭包,保 `tokio::spawn` 所需的 `Send`)。
- `crates/parse-book-source/book-source.schema.json`：随类型重生成(`--features schema`)。
- 书源数据：`fanqie-web.v2.json` explore 接入书库(单页 render);`booksource-generator` skill 文档补充 explore 渲染 + `page` 基数约定。
- 风险:render explore 每翻一页开一次浏览器(秒级,SPA 固有,池化留后续);番茄 search 翻页 / 动态总页数本 change 不覆盖(留后续 change)。
