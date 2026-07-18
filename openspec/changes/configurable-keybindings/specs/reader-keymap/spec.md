# reader-keymap

## ADDED Requirements

### Requirement: 阅读页按键经语义 action 分发
阅读页（`read_novel` 子树：正文滚动/翻页/翻章、Home/End、音量增减、播放暂停、标题显隐，及目录、TTS 面板、信息浮层入口键）SHALL 将按键事件转换为 `KeyCombination` 后向 `reader` scope 反查 action 并按 action 分发；MUST NOT 再直接 match 物理 `KeyCode`。既有行为门控（`is_scroll`、边界二次确认 `Edge`、TTS 状态复位、全书边界 `has_prev`/`has_next`）MUST 原样保留。

#### Scenario: 默认键行为不变
- **WHEN** 无用户配置时在阅读页按 j/k、←/→、PageUp/PageDown、Home/End、+/-、p、v 等默认键
- **THEN** 各按键行为与迁移前逐键一致（含章末/章首二次确认、全书边界提示、TTS 播放暂停）

#### Scenario: 用户重绑翻页键后生效
- **WHEN** 用户配置 `page_down = ["ctrl+d"]` 与 `page_up = ["ctrl+u"]` 后在阅读页按 ctrl+d
- **THEN** 正文向下翻一整屏，PageDown 不再触发翻页

#### Scenario: 未绑定键保持忽略
- **WHEN** 在阅读页按下 `reader` scope 内未绑定任何 action 的键
- **THEN** 事件处理器返回 Ignored，事件继续按既有优先级传给其他处理器（如 shell 键）

### Requirement: 阅读页帮助与提示显示实际绑定
阅读页快捷键帮助浮层条目、底部操作提示及边界确认提示中的键名 SHALL 从当前生效的 keymap 动态取得并格式化显示；用户覆盖某 action 的键位后，上述界面 MUST 显示新键名而非默认键名。

#### Scenario: 帮助浮层反映用户覆盖
- **WHEN** 用户把向下翻页重绑为 ctrl+d 后打开阅读页快捷键帮助浮层
- **THEN** 「下一页」条目显示 ctrl+d，而非 PageDown

#### Scenario: 边界提示反映用户覆盖
- **WHEN** 用户把向下滚动重绑为其他键并滚动到章末触发二次确认提示
- **THEN** 提示文案中的键名为用户绑定的键，而非默认的 ↓
