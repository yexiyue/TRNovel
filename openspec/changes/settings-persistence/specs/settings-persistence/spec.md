# settings-persistence

## ADDED Requirements

### Requirement: 设置在任意入口修改后都会持久化

用户在任何能修改设置的页面（阅读页浮层、全局设置页、主题设置页）改动设置后，值必须写入
`config.toml`，包括切页或退出应用前不足防抖间隔的最后一次改动；重启后读回同一值。

#### Scenario: 全局设置页改阅读偏好后直接退出

- **WHEN** 用户在全局设置页把「每行字数」从跟随窗口调到 10 字，随后直接按 q 退出
- **THEN** `config.toml` 的 `[reader] lines_per_row` 为 10
- **AND** 重新启动进入全局设置页仍显示「10 字(20 列)」

#### Scenario: 写盘失败不吞掉改动

- **WHEN** 保存时磁盘写入失败
- **THEN** 已保存快照不推进，下一次改动会连同这次一起重试
- **AND** 界面不因写盘失败而中断

### Requirement: 设置字段无遗漏地读写

`ReaderDisplayConfig` 的每个字段与 `AppearanceConfig` 的每个字段（含自定义主题的逐色
覆盖表）都必须能经 TOML 序列化、落盘、重新加载后保持一致。

#### Scenario: 全字段往返

- **WHEN** 每个字段都设为非默认值后保存并重新加载
- **THEN** 加载结果与保存前逐字段相等
