## ADDED Requirements

### Requirement: 外观配置必须紧凑且基于命名主题
TRNovel SHALL 将 TUI 外观存储为项目自有配置，配置只包含当前主题 slug 与背景模式。系统 MUST NOT 持久化原始 `Palette`、内置组件 theme 结构，或旧的 `ThemeConfig` 派生样式树。

#### Scenario: 加载已保存外观
- **WHEN** TRNovel 启动，并且新的外观缓存中包含有效主题 slug 与背景模式
- **THEN** 应用首帧 TUI 使用该命名主题与背景模式

#### Scenario: 缺失或无效外观回退默认值
- **WHEN** TRNovel 启动时没有新的外观缓存，或缓存中的主题 slug 无法识别
- **THEN** 应用使用默认外观，并且启动不失败

#### Scenario: 外观变更会保存并立即生效
- **WHEN** 用户选择不同命名主题或背景模式
- **THEN** 应用写入新的紧凑外观配置，并在不重启的情况下更新当前 TUI

### Requirement: PaletteProvider 必须驱动整个 TUI
TRNovel SHALL 从当前外观派生 ratatui-kit `Palette`，并通过 `PaletteProvider` 提供给应用子树。应用 MUST 在根 TUI surface 显式应用 palette 背景。

#### Scenario: 切换主题会更新共享 UI
- **WHEN** 当前外观从一个命名主题切换到另一个命名主题
- **THEN** 内置组件、自定义组件、页面、模态、搜索框和列表选中态在下一帧从新 palette 渲染

#### Scenario: 主题背景模式填充根背景
- **WHEN** 背景模式为 `Theme`
- **THEN** 根 TUI surface 使用命名主题的背景色

#### Scenario: 终端背景模式保留终端背景
- **WHEN** 背景模式为 `Terminal`
- **THEN** 根 TUI surface 保留终端背景，同时继续使用命名主题的前景色、强调色、选中色与语义色

### Requirement: 主题设置必须使用命名主题
TRNovel SHALL 用基于 `ratatui-kit-themes` 的命名主题设置 UI 替换手工六色主题编辑。设置 UI MUST 允许用户选择主题、切换背景模式、预览当前 palette，并重置为默认值。

#### Scenario: 用户选择命名主题
- **WHEN** 用户在设置列表中选择一个主题
- **THEN** TRNovel 立即应用并保存该主题

#### Scenario: 用户切换背景模式
- **WHEN** 用户在主题背景与终端背景之间切换
- **THEN** TRNovel 立即应用并保存所选背景模式

#### Scenario: 用户重置外观
- **WHEN** 用户在主题设置页确认重置
- **THEN** TRNovel 恢复并保存默认命名主题与默认背景模式

#### Scenario: 手工取色器不可用
- **WHEN** 用户打开主题设置页
- **THEN** 页面展示命名主题与背景控制，而不是旧的六色取色器

### Requirement: 组件必须消费框架主题协议
TRNovel UI 代码 SHALL 通过 ratatui-kit 内置组件主题、`use_palette` 或项目自定义 `ComponentTheme` 消费主题数据。应用组件与页面 MUST NOT 依赖 `ThemeConfig`、`ThemeColors`、`UseThemeConfig` 或直接的 `theme.*` 样式槽。

#### Scenario: 共享组件从 palette 渲染
- **WHEN** 搜索框、单选列表、多选列表、文件选择、加载状态、确认弹窗、警告弹窗和快捷键信息弹窗渲染
- **THEN** 它们的文本、边框、高亮、空态和语义状态来自当前 palette 或项目 `ComponentTheme`

#### Scenario: 领域渲染器不携带 ThemeConfig
- **WHEN** 页面构建历史记录、搜索结果、书源、章节列表或阅读正文的自定义行/详情文本
- **THEN** 这些渲染器不需要 `ThemeConfig` prop，而是在组件作用域内从当前 palette/theme 派生样式

#### Scenario: 静态主题模型被移除
- **WHEN** 实现完成
- **THEN** 在 `src/` 中搜索 `ThemeConfig`、`ThemeColors`、`UseThemeConfig` 或 `theme.` 样式槽用法，不应找到活跃的主题系统依赖

### Requirement: 阅读显示偏好必须独立于外观
TRNovel SHALL 将阅读显示偏好持久化在 TUI 外观配置之外。阅读标题显示开关 MUST NOT 存储在主题选择中、从主题选择中加载，或被主题选择修改。

#### Scenario: 标题显示独立切换
- **WHEN** 用户在阅读时切换标题显示
- **THEN** TRNovel 更新并保存阅读显示偏好，且不改变当前外观

#### Scenario: 主题切换保留标题显示偏好
- **WHEN** 用户改变命名主题或背景模式
- **THEN** 阅读标题显示偏好保持不变

#### Scenario: 启动时加载阅读偏好
- **WHEN** TRNovel 启动且存在已保存的阅读显示偏好
- **THEN** 阅读页渲染章节标题与底部状态时使用该偏好

### Requirement: 旧主题文件不得迁移
TRNovel SHALL 将旧六色 `theme.json` 格式视为本次重构后的不支持格式。旧主题文件存在时 MUST NOT 破坏启动，也 MUST NOT 影响当前外观，除非用户已经保存了新的外观配置。

#### Scenario: 只有旧主题文件
- **WHEN** TRNovel 启动时只存在旧 `theme.json`
- **THEN** 应用忽略旧文件并使用默认外观

#### Scenario: 旧主题文件与新外观文件同时存在
- **WHEN** TRNovel 启动时旧 `theme.json` 与新外观缓存同时存在
- **THEN** 应用使用新外观缓存并忽略旧文件

### Requirement: 文档必须描述新主题模型
TRNovel 文档 SHALL 描述命名主题、背景模式、重置行为，以及旧手工取色器的 breaking removal。

#### Scenario: 用户阅读主题指南
- **WHEN** 用户打开主题指南
- **THEN** 指南说明如何选择命名主题、切换背景模式、重置外观，并说明旧六色编辑不再可用
