## ADDED Requirements

### Requirement: explore 渲染取页通道
`explore` 的请求配置标 `render: true` 且提供 `readyFor` 或 `interceptApi` 时,系统 SHALL 用受控浏览器渲染分类 URL(执行站点自身 JS),取「渲染后 DOM」或「CDP 拦截的 API 响应体」作为取页结果交给现有 `list`/`item` 规则——与 `search` 主请求的渲染语义一致;`explore` 不开 `render` 时 MUST 保持纯 reqwest 直取(现状)。系统 MUST NOT 复刻任何站点签名(由站点 JS 自行完成)。

`explore`/`search` 取页 SHALL 为**单页**:把分类/搜索 URL 模板里的 `{{page}}` 解析为调用方传入的 `page`、取该页、抽取并返回。交互式翻页由调用方(UI)**递增 `page` 重新调用**驱动;引擎 MUST NOT 自行批量翻多页(避免与 UI 翻页机制冲突、避免一次渲染连开多个浏览器)。

#### Scenario: explore 拦截分类 API 的 JSON
- **WHEN** `explore` 标 `render:true` + `interceptApi:"book_list/v0"`,SPA 加载分类页后自行签名请求该接口
- **THEN** 取页结果为该响应体 JSON,`list`/`item` 的 `via:"json"` 规则能从中取出书列表

#### Scenario: explore 未开 render 保持现状
- **WHEN** `explore` 无 `render`(或为 false)
- **THEN** `engine.explore` 走 reqwest `fetch_checked` 直取,行为与本 change 之前逐字节一致

#### Scenario: 单页取页 + page 映射 URL(交互翻页由 UI 递增 page 驱动)
- **WHEN** 分类 URL 模板含 `{{page}}`,调用 `explore(category, page=2)`
- **THEN** 系统只取第 2 页(`{{page}}` 解析为 2,如 `/library/all/page_2`),返回该页书列表;不批量翻多页

#### Scenario: 渲染失败优雅降级
- **WHEN** 浏览器不可用、未授权、或超时内未拦到/未渲染出目标内容
- **THEN** 该 op 取页失败并优雅降级(不 panic、不影响其它 op),诊断指明渲染/拦截失败
