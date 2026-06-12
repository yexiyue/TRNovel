## ADDED Requirements

### Requirement: 点击驱动翻页(pageBy.click)
当 render 拦截型取页的请求配 `pageBy.click: "<选择器>"` 且调用方传入 `page > 1` 时,系统 SHALL 在**一次受控浏览器活页**内,从首页起点击该「下一页」选择器 `page - 1` 次,每次点击后拦截目标 API 响应(URL 含该请求的 `interceptApi`、并按目标 `page_index` 对齐,见「稳健点击投递与前进断言」),并把**第 `page` 页**的 API 响应体作为取页结果交给现有 `list`/`item` 规则。`pageBy` 缺席(或无 `interceptApi`、或 `page == 1`)时系统 MUST 保持单页取页(现状,翻页行为逐字节不变)。若 `page` 超过实际总页数,点击循环 SHALL 在末页自然停止并返回末页结果(见「点击翻页到头即停」),MUST NOT 报错。系统 MUST NOT 复刻任何站点签名——每页请求由站点 JS 自行签名。引擎 MUST NOT 跨调用持有活页:活页仅在本次取页内存活,交互式翻页由 UI 递增 `page` 重新调用驱动。

#### Scenario: 点击翻到第 2 页(实测直接观测的一跳)
- **WHEN** `search` 配 `render:true` + `interceptApi:"search_book/v1"` + `pageBy.click:"<next 选择器>"`,调用 `search(key, page=2)`,SPA 加载首页后
- **THEN** 系统点「下一页」1 次,拦到 `page_index=1` 的 `search_book/v1` 响应,返回第 2 页书列表(与第 1 页内容不同)

#### Scenario: 点击翻到更深页(多跳,线性)
- **WHEN** 调用 `search(key, page=3)`
- **THEN** 系统从首页点「下一页」2 次,每次按目标 `page_index` 对齐拦响应,采纳第 3 跳(`page_index=2`)的响应、返回第 3 页书列表(多跳线性翻页;深页稳定性见本 change 的深页验收项)

#### Scenario: pageBy 缺席保持现状
- **WHEN** 请求无 `pageBy`(或无 `interceptApi`)
- **THEN** 走现状单拦截路径(`render_intercept`),翻页行为与本 change 之前逐字节一致

#### Scenario: 第 1 页不点击
- **WHEN** 配了 `pageBy.click` 但调用 `page == 1`
- **THEN** 只拦首页 API 响应、不执行任何点击

### Requirement: 稳健点击投递与前进断言
执行「下一页」点击时,系统 SHALL 确保点击真实送达控件(必要时先将控件滚动入视、派发真实指针事件)。系统 SHALL 以「该次点击**之后**到达、URL 含 `interceptApi`、且按**期望 `page_index`(= `page - 1`,从响应 URL query 解析)对齐**的新响应」为页已前进的判据;当 `interceptApi` 响应携带页码参数时,`page_index` 对齐为 MUST(以排除软封锁 reload 注入的残留 `page_index=0` 重取响应)。系统 MUST NOT 假设「一次点击=翻一页」,也 MUST NOT 采纳点击之前已存在的旧响应。点击未令页前进且控件仍可点(非到头)时,系统 SHALL 重试点击(或滚动入视后重派事件),仍未前进则在该页等待超时后终止。

#### Scenario: 折叠下方的控件仍被点中
- **WHEN** 「下一页」控件渲染在视口可见区之外(页面很长)
- **THEN** 系统先滚动控件入视并派发真实点击,页成功前进(不因控件离屏而静默失败)

#### Scenario: 只采纳点击之后、page_index 对齐的响应
- **WHEN** 点击之前存在残留的旧目标响应(如软封锁 reload 触发的 `page_index=0`)
- **THEN** 系统只采纳点击**之后**到达、且 `page_index` == 目标页-1 的响应作为新页结果,丢弃残留的 `page_index=0` 与任何点击前已见响应

#### Scenario: 点击已投递但页未前进则重试
- **WHEN** 一次点击已投递、但控件未禁用(非到头),目标响应在该页超时内未到达(点击未生效 / 拥塞)
- **THEN** 系统重试点击(滚动入视后重派真实事件);重试耗尽仍未达目标页则取页**失败(报错)**,交上层重试 / 优雅降级,**MUST NOT 把更早的页当作第 N 页返回**(仅控件禁用/缺失的「真到头」才返回当前末页)

### Requirement: 点击翻页到头即停
点击翻页循环 SHALL 在到达末页时停止并返回当前页结果,而非报错或无限点击。**超时 SHALL 为正确性保证**:点击「下一页」后在超时内不再到达按目标 `page_index` 对齐的新响应,即视为到头。当「下一页」控件暴露禁用态(如禁用样式 class)时,系统 SHOULD 以该结构态作主快路提前判定到头(免每个末页空等一次超时),并据此区分「真到头(控件禁用)」与「点击失败 / 通道拥塞(控件可点但未前进)」——后者 MUST 按「稳健点击投递与前进断言」重试,MUST NOT 当作到头。

#### Scenario: 末页控件禁用即停(结构快路)
- **WHEN** 已位于末页,「下一页」控件带禁用态(如 `disabled` class)
- **THEN** 系统据禁用态即时判定到头,停止循环、返回末页结果,不再点击、不空等超时

#### Scenario: 控件可点但无响应到达即停(超时兜底)
- **WHEN** 末页判据不可由结构态得出,点击「下一页」后超时内无对齐的新响应到达
- **THEN** 系统等待超时后终止点击循环,返回当前页结果,不报错、不无限点击

### Requirement: 软封锁 reload-once 恢复
当渲染拦截到**空响应体**(或检出站点的空结果 / 验证码软封锁信号)时,系统 SHALL 自动 reload 当前页一次后重试,再据重试结果判定;reload 后仍空才作零结果 / 失败优雅降级。此恢复 SHALL 同样适用于**第 1 页取页的现有单拦截路径**(不限点击翻页场景),且 MUST NOT panic 或卡死;因空 body 本即失败态,reload-once 严格改善失败态、MUST NOT 改变原本非空(成功)的结果。

#### Scenario: 首页软封锁经 reload 恢复
- **WHEN** `search` 首次加载,目标 `search_book/v1` 返回 HTTP 200 但 body 为空(撞软封锁 / 验证码)
- **THEN** 系统 reload 当前页一次,取到非空结果后正常返回书列表

#### Scenario: reload 后仍空作零结果
- **WHEN** reload 重试后目标响应 body 仍为空
- **THEN** 系统优雅作零结果 / 取页失败(明确诊断),不 panic、不卡死、不影响其它 op

#### Scenario: 非空响应不触发 reload(不改成功路径)
- **WHEN** 首次拦截到的目标响应 body 非空
- **THEN** 系统直接采纳该结果、不 reload,行为与本 change 之前一致(reload-once 仅作用于空 body 失败态)
