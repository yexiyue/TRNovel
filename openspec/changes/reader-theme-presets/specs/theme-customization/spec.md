# theme-customization

## ADDED Requirements

### Requirement: 自定义配色持久化
外观配置 SHALL 支持 `custom_themes` 列表，每项包含中文名称、稳定 slug、基于的主题 slug，以及 `colors` 映射（Palette 字段名 → `#rrggbb`）。该字段 MUST 使用 serde 默认值，旧配置缺失时按空列表加载。颜色键 MUST 与 Palette 字段名一一对应，未知键与非法颜色 MUST 被忽略并回落到 base；旧版仅有 `bg`/`fg`/`accent` 的项 MUST 仍能解析并映射为对应覆盖，`colors` 优先。自定义 slug SHALL 采用 `custom-<n>` 形式，生成时避开已有自定义、预设与内置主题的 slug。

#### Scenario: 旧配置照常加载
- **WHEN** 旧版 `config.toml` 的 `[appearance]` 没有 `custom_themes`
- **THEN** 配置加载成功，自定义列表为空，原主题与终端背景行为不变

#### Scenario: 旧方案三色兼容
- **WHEN** 某自定义项只有 `bg`/`fg`/`accent`，没有 `colors`
- **THEN** 三项映射为对应颜色覆盖，其余字段沿用 base，程序不报错

#### Scenario: 未知颜色键被忽略
- **WHEN** `colors` 中出现 `not_a_palette_field = "#ffffff"`
- **THEN** 该键被静默忽略，配色仍按其余合法字段与 base 解析

#### Scenario: 重启后仍可用
- **WHEN** 用户保存自定义方案并重启程序
- **THEN** 方案仍出现在主题列表的「自定义」分组，选中后颜色与保存时一致

### Requirement: 新建与二次编辑
主题设置页 SHALL 允许按 `N` 打开新建弹窗，并对当前选中的自定义方案按 `E` 进入编辑。编辑弹窗 MUST 预填该方案的名称、基准主题与全部颜色；保存时 MUST 覆盖同一个 slug 与原列表位置，而不是追加新项。`N` 仍 SHALL 生成新的 `custom-<n>`。弹窗标题 MUST 区分「新建方案 / 编辑方案」。预设与内置主题 MUST NOT 进入编辑。

#### Scenario: 编辑现有方案
- **WHEN** 当前选中 `custom-1`，用户按 E
- **THEN** 弹窗预填其名称、base 与当前全部颜色，标题为「编辑方案」

#### Scenario: 保存覆盖同一方案
- **WHEN** 用户在编辑弹窗中改色并按 Enter
- **THEN** `custom-1` 被原位更新，列表不新增条目，界面立即应用新颜色

#### Scenario: 预设不可编辑
- **WHEN** 当前选中的是「豆沙绿」预设
- **THEN** 按 E 不打开编辑弹窗，也不会修改配置

### Requirement: 逐字段取色与用途说明
编辑弹窗 SHALL 按 Palette 的 15 个字段逐项展示：中文标签、当前色块、`#rrggbb` 值和使用场景说明。颜色列表 MUST 可滚动且高度不超过弹窗。`Tab`/`Shift+Tab` SHALL 在名称、基于哪套、颜色列表之间循环；`↑`/`↓` SHALL 在颜色列表中移动；`←`/`→` SHALL 在固定 16 色中循环取色；用户 MAY 直接输入 `#rrggbb`，输入完整且合法时立即更新当前字段。`Enter` 保存，`Esc` 取消。

#### Scenario: 滚动 15 个字段
- **WHEN** 用户按 ↓ 越过列表可见的最后一项
- **THEN** 列表滚动到下一段，当前字段与色块始终可见，弹窗尺寸不变

#### Scenario: 手输 hex
- **WHEN** 用户在某个颜色项输入合法的 `#1e90ff`
- **THEN** 当前字段色块与 `#rrggbb` 立即更新，保存后写入 `colors`

#### Scenario: 非法 hex 不写回
- **WHEN** 用户输入的 `#zzzzzz` 不合法
- **THEN** 当前颜色保持原值，保存时不会写入非法字符串

#### Scenario: 未改字段沿用 base
- **WHEN** 用户只改了「正文」一个字段并保存
- **THEN** 其余 14 个字段仍与基准主题一致，`colors` 中只出现正文键

### Requirement: 自定义方案覆盖 base 语义色
自定义方案的色板 SHALL 以 `base` 对应主题的完整色板为底，按 `colors` 逐项覆盖；未填字段 MUST 沿用 base。未知 base SHALL 回落到默认主题而不是拒绝启动。`on_accent` 与 `border_active` 仅在用户未显式覆盖时随强调色重算；显式覆盖 MUST 被尊重。

#### Scenario: 只改一个颜色
- **WHEN** 某自定义方案基于「米黄纸」，只覆盖了正文色
- **THEN** 选中态、成功、警告等其余字段仍与米黄纸一致

#### Scenario: 显式覆盖派生色
- **WHEN** 用户同时设置了强调色与 `on_accent`
- **THEN** `on_accent` 使用用户值，不被强调色自动重算覆盖

#### Scenario: base 不存在
- **WHEN** 配置中的 base 指向已删除或不存在的主题
- **THEN** 以默认主题色板为底继续应用该方案，程序不崩溃

### Requirement: 删除自定义方案
主题列表 SHALL 允许按 `X` 删除当前选中的自定义方案，并先弹出二次确认。预设与内置主题 MUST NOT 被删除。删除正在使用的方案后 SHALL 回落到默认主题，且配置文件中不再包含该项。

#### Scenario: 删除当前方案
- **WHEN** 当前主题为 `custom-1`，用户按 X 并确认
- **THEN** 该方案从列表与配置中移除，主题切回默认主题，界面颜色立即更新

#### Scenario: 取消删除
- **WHEN** 用户按 X 后选择取消
- **THEN** 方案保留，选中项与当前主题不变

#### Scenario: 不能删除预设
- **WHEN** 当前选中的是「豆沙绿」预设
- **THEN** 按 X 不打开删除确认，也不会修改配置
