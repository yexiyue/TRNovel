# reader-display

## ADDED Requirements

### Requirement: 版心宽度可调
阅读正文的折行宽度 SHALL 由「终端宽度 − 2×边框宽度 − 2×左右留白」与「每行字数」取小决定；每行字数为 `0` 时表示跟随终端宽度，不再额外限制。每行字数的合法范围为 `10..=200`，越界值 SHALL 在配置加载时钳制到边界而不是拒绝启动。

#### Scenario: 限制每行字数
- **WHEN** 用户把每行字数设为 30，终端宽 120 列
- **THEN** 正文每行最多显示 30 个汉字，两侧留出空白，而不是铺满 120 列

#### Scenario: 跟随终端
- **WHEN** 每行字数为 `0`
- **THEN** 折行宽度等于可用宽度，立即跟随终端缩放变化

#### Scenario: 脏值钳制
- **WHEN** 配置文件里的每行字数是 `3` 或 `999`
- **THEN** 加载后分别变成 `10` 与 `200`，程序正常启动

### Requirement: 段落首行缩进
正文 SHALL 支持为每个逻辑段落的第一个可视行加缩进，缩进字符数范围 `0..=8`。缩进 MUST NOT 修改正文原始文本，也 MUST NOT 影响搜索高亮的字节偏移与 TTS 朗读的定位。

#### Scenario: 缩进只作用于段首
- **WHEN** 首行缩进设为 2，某段落折成 3 行
- **THEN** 只有第一行前有 2 个空格，后两行不缩进

#### Scenario: 不影响搜索
- **WHEN** 开启首行缩进后搜索正文关键词
- **THEN** 命中位置与未缩进时完全一致，高亮范围正确

### Requirement: 段间距可调
段落之间补的空行数 SHALL 由 `paragraph_gap` 控制，范围 `0..=4`，默认 `0`。旧配置字段 `paragraph_spacing` SHALL 继续被读取：为 `true` 时等价于补 1 行；两个字段在写盘时 SHALL 保持 `paragraph_spacing == (paragraph_gap > 0)` 的一致关系。原文自带的空行 MUST NOT 被删除，也 MUST NOT 与自动补的空行叠加。

#### Scenario: 旧配置升级
- **WHEN** 旧配置里 `paragraph_spacing = true` 且没有 `paragraph_gap`
- **THEN** 加载后 `paragraph_gap = 1`，正文段落之间仍有空行

#### Scenario: 关闭段间距
- **WHEN** 用户把段间距减到 0
- **THEN** 段落之间不再补空行，`paragraph_spacing` 同步变为 `false`

### Requirement: 左右留白
正文 SHALL 支持左右各留白的列数，范围 `0..=6`。留白计入可用宽度，折行按留白后的宽度计算，正文 MUST NOT 压到边框上。

#### Scenario: 增加留白
- **WHEN** 用户把左右留白设为 3
- **THEN** 正文左右各空出 3 列，折行宽度相应减少 6 列

### Requirement: 阅读页边框可选
阅读页边框样式 SHALL 支持 `single`、`rounded`、`double`、`thick`、`none`，默认 `single`。`none` 时正文 MUST 铺满整个可用区域，并因此多出 2 行可见高度。边框上的标题 SHALL 由 `show_frameline_title` 控制，关闭或 `none` 时不显示。

#### Scenario: 关闭边框
- **WHEN** 用户把边框切到「无边框」
- **THEN** 正文四周的框线消失，可见行数比单线边框时多 2 行

#### Scenario: 隐藏边框标题
- **WHEN** 用户关闭「边框标题」
- **THEN** 上边框不再显示章节名，下边框不再显示进度，框线本身保留

### Requirement: 常显快捷键提示条
阅读页 SHALL 在正文与状态栏之间常显一行快捷键提示，默认开启，可由 `show_hint_bar` 关闭。提示内容 SHALL 由当前键位映射动态生成（翻页、上一章、下一章、搜索、目录、设置、帮助），按终端显示宽度裁剪：放不下的条目整段丢弃，末尾空间足够时才补省略号。关闭提示条或终端过窄无内容时，该行 MUST 占用 0 行高度。

#### Scenario: 重绑键位后提示跟随
- **WHEN** 用户把「翻页」改为 `space`
- **THEN** 提示条显示 `space 翻页` 而不是旧键名

#### Scenario: 窄终端不溢出
- **WHEN** 终端宽度缩到 20 列
- **THEN** 提示条只显示放得下的条目，不会换行、不会越界覆盖状态栏

#### Scenario: 关闭提示条
- **WHEN** 用户关闭「快捷键提示」
- **THEN** 该行消失，可见正文多 1 行

### Requirement: 可见行数统一计算
正文可见行数 SHALL 由「终端高度 − 状态栏 1 行 − 2×边框宽度 − 提示条占用行数」统一计算，并至少为 1。翻页、进度显示、设置面板预览 SHALL 使用同一结果。

#### Scenario: 提示条与无边框叠加
- **WHEN** 提示条开启（1 行）且边框为 `none`
- **THEN** 可见行数 = 终端高度 − 2（状态栏 + 提示条），末行完整可见
