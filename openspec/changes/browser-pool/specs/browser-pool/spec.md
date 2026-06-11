## ADDED Requirements

### Requirement: 渲染取页复用常驻浏览器实例
渲染取页(`render_intercept`/`render_dom`)时,系统 SHALL 复用一个常驻浏览器实例:首次渲染懒启动并保留,后续渲染只开新 `Page` 导航目标 URL、用完关 `Page` 而保留 `Browser`,不再每次 launch/close 整个浏览器。串行语义(`BROWSER_LOCK`)、失败熔断(`RENDER_FAILED`)、对外接口签名与优雅降级 MUST 保持不变。

#### Scenario: 多次渲染只启动一次浏览器
- **WHEN** 同一会话内连续多次渲染取页(如 explore 翻页 page 1→2→3)
- **THEN** 仅首次 launch 浏览器,后续复用同一 `Browser`、各开一个新 `Page`,翻页延迟显著低于每次重启

#### Scenario: 浏览器断连后重建
- **WHEN** 常驻浏览器崩溃 / 断连(handler 结束 / CDP 调用失败)后再次渲染取页
- **THEN** 系统丢弃失效实例、重新 launch 一个,本次取页正常完成(或按现有失败语义降级)

#### Scenario: 空闲 / 退出释放
- **WHEN** app 退出(或可选的空闲超时到达)
- **THEN** 系统关闭常驻浏览器与其 handler task,不残留僵尸进程

#### Scenario: 无浏览器仍优雅降级
- **WHEN** 浏览器不可用 / 未授权
- **THEN** 渲染取页失败并优雅降级(不 panic、不影响其它 op),与本 change 之前一致
