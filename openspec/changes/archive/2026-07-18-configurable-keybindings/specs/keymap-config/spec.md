# keymap-config

## ADDED Requirements

### Requirement: 默认键位表与现状逐键一致
系统 SHALL 在代码内为每个 scope 的每个 action 声明默认键位；在不存在用户配置文件时，所有按键行为 MUST 与引入本能力前的硬编码键位逐键一致。

#### Scenario: 无配置文件启动
- **WHEN** `~/.novel/keybindings.toml` 不存在时启动应用
- **THEN** 所有页面按键行为与引入本能力前完全一致，且不产生任何告警

### Requirement: 从 keybindings.toml 加载用户覆盖
系统 SHALL 在启动初始化时读取 `~/.novel/keybindings.toml`（TOML 格式，scope 为表名、action 为键、键位字符串列表为值，键位语法为 crokey 语法），并将用户条目按 action 整条替换合并到默认表之上；未出现在配置中的 action MUST 保持默认键位。该文件对程序 MUST 为只读（程序不写回、不重排、不删注释）。

#### Scenario: 部分覆盖
- **WHEN** 配置文件仅包含 `[reader]` 下的 `page_down = ["ctrl-d"]`
- **THEN** 阅读页 `ctrl-d` 触发向下翻页，默认的 PageDown 键不再触发翻页，其余所有 action 保持默认键位

#### Scenario: 一个 action 绑定多个键
- **WHEN** 配置文件写 `page_down = ["ctrl-d", "space"]`
- **THEN** 两个键都触发向下翻页

### Requirement: 非法配置回退默认并告警
系统 SHALL 校验用户配置：键位字符串无法解析时该 action 回退默认；同一 scope 内两个 action 绑定同一键时，冲突的用户覆盖整条回退默认；整个文件 TOML 解析失败时全部使用默认表。所有校验问题 MUST 收集为告警并在启动后通过 WarningModal 一次性呈现，MUST NOT 阻断启动或导致 panic。

#### Scenario: 键位字符串非法
- **WHEN** 配置中某 action 的键位写为无法解析的字符串（如 `"ctrl-"`）
- **THEN** 该 action 使用默认键位，启动后弹出告警说明该条目无效，应用正常可用

#### Scenario: 同 scope 键位冲突
- **WHEN** 用户在 `[reader]` 内把两个不同 action 绑定到同一个键
- **THEN** 这两个 action 的用户覆盖均回退默认键位，启动后弹出告警指出冲突的键

#### Scenario: 文件整体损坏
- **WHEN** `keybindings.toml` 存在但不是合法 TOML
- **THEN** 全部 action 使用默认键位，启动后弹出告警，应用正常可用

### Requirement: Keymap 通过全局 Atom 暴露且支持反查
系统 SHALL 将合并后的 Keymap 存入全局 `KEYMAP: Atom<Keymap>`（启动加载后运行期只读），并提供按 scope 的 `KeyCombination → action` 反查接口与按 action 的键名列表接口（供帮助界面渲染显示字符串）。

#### Scenario: 事件分发查表
- **WHEN** 页面事件处理器收到一个按键事件并向所属 scope 反查
- **THEN** 命中绑定时返回对应 action，未命中时返回空（事件按 Ignored 处理）

#### Scenario: 帮助渲染取键名
- **WHEN** 帮助界面请求某 action 的键名列表
- **THEN** 返回按当前生效绑定（含用户覆盖）格式化的可读键名字符串
