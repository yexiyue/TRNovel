## ADDED Requirements

### Requirement: 渲染后 DOM 取页
书源某 op 的 `request` 标 `render: true` 且提供 `readyFor`(CSS 就绪选择器)时,系统 SHALL 用受控浏览器导航该 URL、执行站点自身 JS、轮询直到 `readyFor` 选择器出现(或超时),然后把渲染后的 DOM(`document.documentElement.outerHTML`)作为取页结果交给现有抽取规则;签名/请求由站点自身 JS 完成,系统 MUST NOT 复刻任何站点签名。

#### Scenario: 就绪后返回渲染 DOM
- **WHEN** op 标 `render:true` + `readyFor:".result-item"`,浏览器导航后页面 JS 在超时内渲染出 `.result-item`
- **THEN** 取页结果为渲染后 DOM,后续 `list`/`item` 的 CSS 规则能在其上抽取到数据

#### Scenario: 就绪超时降级
- **WHEN** 超时内 `readyFor` 选择器始终未出现
- **THEN** 该 op 取页失败并优雅降级(不 panic、不影响其它 op),诊断信息指明「渲染就绪超时」

### Requirement: CDP 拦截 API 响应取页
书源某 op 的 `request` 标 `render: true` 且提供 `interceptApi`(目标响应 URL 子串/正则)时,系统 SHALL 用受控浏览器导航该 URL,在浏览器内通过 CDP 网络域拦截 URL 匹配 `interceptApi` 的响应体并作为取页结果(供 `via:"json"` 规则解析);用于「结果只在签名 API、DOM 无关键字段(如 book_id)」的 SPA 站点。

#### Scenario: 拦截到签名 API 的 JSON 响应
- **WHEN** op 标 `render:true` + `interceptApi:"search_book/v1"`,SPA 加载后自行签名并请求该接口
- **THEN** 系统拦截到该响应体 JSON,`list`/`item` 的 `via:"json"` 规则可从中取出书列表(含 `book_id` 等明文字段)

#### Scenario: 未拦截到则降级
- **WHEN** 超时内未出现匹配 `interceptApi` 的响应(被拒签/网络失败)
- **THEN** 该 op 取页失败并优雅降级,诊断指明「未拦截到目标响应」

### Requirement: Headless 渲染策略
渲染/拦截取页 SHALL 默认使用无头(headless)浏览器;仅书源登录与解 Cloudflare/Turnstile 挑战(需真人交互或反检测)使用有头(headful)。渲染失败时系统 SHALL 优雅降级而非阻塞主阅读链路。

#### Scenario: 渲染走无头
- **WHEN** engine 为渲染型 op 取页
- **THEN** 浏览器以无头方式启动(无可见窗口),取页结果与有头一致

#### Scenario: 登录仍走有头
- **WHEN** 触发书源登录或撞 CF/Turnstile 挑战
- **THEN** 浏览器以有头方式启动,供用户交互(行为同现状)

### Requirement: 按 op 显式开启且向后兼容
渲染取页 SHALL 仅在 op 的 `request` 显式 `render: true` 时启用;`render` 默认 false。未开启渲染的 op、以及所有现有书源 JSON,行为 MUST 与现状逐字节一致(reqwest 直取)。本能力 MUST 仅在 `browser` feature 下编译;纯净构建/无浏览器环境行为不变。

#### Scenario: 现有书源零行为变化
- **WHEN** 加载未含 `render` 字段的现有书源 JSON
- **THEN** 解析与取页行为与本 change 前完全一致

#### Scenario: 无浏览器环境降级
- **WHEN** 书源开启渲染但运行环境无可用浏览器(或纯净构建)
- **THEN** 渲染 op 不可用并优雅降级,其它 op(reqwest 可取的)照常工作
