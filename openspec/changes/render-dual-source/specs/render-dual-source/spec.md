## ADDED Requirements

### Requirement: render 拦截取页暴露 API JSON 与渲染 DOM 双源
render 取页用 `interceptApi` 时,**当本 op 配置了 `via:css`/`via:xpath` 的源规则**(如 DOM 版 `totalPages`),系统 SHALL 在拦到目标 API 响应体后,(若配 `ready_for` 则先等该选择器出现)抓取渲染后 DOM(`outerHTML`),与拦截的 API JSON 一并返回。规则求值 SHALL 按 `Rule.via` 路由:`via:json` 求值 API JSON、`via:css`/`via:xpath` 求值 DOM。未配置任何 DOM 源规则时 MUST 不抓 DOM(零额外开销,与现状逐字节一致)。`intercept_api` 与 `ready_for` MAY 共存(前者取数据、后者作 DOM 就绪闸)。

#### Scenario: 拦 API 取列表 + 读 DOM 取总页数
- **WHEN** `explore` 配 `interceptApi:"book_list/v0"` + `ready_for:".paginator"` + `totalPages:{via:css, select:".paginator .last"}`,渲染后分页器出现
- **THEN** `list`/`item`(`via:json`)对拦到的 `book_list` JSON 求值得书列表,`totalPages`(`via:css`)对渲染 DOM 求值得总页数

#### Scenario: 未配 DOM 源规则不抓 DOM
- **WHEN** `explore` 只配 `interceptApi`(无 `via:css`/`xpath` 规则)
- **THEN** 系统不抓 `outerHTML`、返回的 DOM 源为 `None`,行为与本 change 之前逐字节一致

#### Scenario: 就绪闸等待分页器
- **WHEN** 配了 `ready_for` 的 DOM 选择器,分页器在 API 响应之后才由 JS 渲染
- **THEN** 系统在有界超时内等该选择器出现再抓 DOM(超时则抓当前 DOM,尽力)

### Requirement: explore/search 返回精确总页数(total_pages)
`explore`/`search` 配置可选 `totalPages` 规则时,系统 SHALL 在单页取页后对该规则 `via` 指向的源(API JSON 或渲染 DOM)求值一次,解析为 `u32` 总页数,随书列表一并返回(`BookList.total_pages: Option<u32>`)——解析失败/空 → `None`。该求值 MUST 复用同一页取页结果,不额外发请求。无 `totalPages` 规则时返回 `None`(现状)。

#### Scenario: 从 DOM 分页器读总页数
- **WHEN** `explore` 配 DOM 版 `totalPages`,渲染 DOM 分页器末页为 `99`
- **THEN** 返回 `total_pages` 为 `Some(99)`,UI 可显示「第 N / 99 页」

#### Scenario: 从 API JSON 读总页数(便宜档,无需 DOM)
- **WHEN** 某书源 `totalPages:{via:json, select:"$.data.total_pages"}`,拦到的 API JSON 含可靠总数
- **THEN** 返回 `total_pages` 为 `Some(M)`,不抓 DOM

#### Scenario: 无 totalPages 规则
- **WHEN** `explore`/`search` 未配 `totalPages`
- **THEN** 返回 `total_pages` 为 `None`,UI 仅显示当前页(与本 change 之前一致)
