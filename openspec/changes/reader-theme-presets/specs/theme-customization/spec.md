# theme-customization

## ADDED Requirements

### Requirement: 自定义配色持久化
外观配置 SHALL 支持 `custom_themes` 列表，每项包含中文名称、稳定 slug、基于的主题 slug 以及底色/正文色/强调色三个 `#rrggbb` 字符串。该字段 MUST 使用 serde 默认值，旧配置缺失时按空列表加载。自定义 slug SHALL 采用 `custom-<n>` 形式，生成时避开已有自定义、预设与内置主题的 slug。

#### Scenario: 旧配置照常加载
- **WHEN** 旧版 `config.toml` 的 `[appearance]` 没有 `custom_themes`
- **THEN** 配置加载成功，自定义列表为空，原主题与终端背景行为不变

#### Scenario: slug 不重复
- **WHEN** 已有 `custom-1`，用户新建第二个方案
- **THEN** 新方案 slug 为 `custom-2`，不会覆盖已有项

#### Scenario: 重启后仍可用
- **WHEN** 用户保存自定义方案并重启程序
- **THEN** 方案仍出现在主题列表的「自定义」分组，选中后颜色与保存时一致

### Requirement: 从现有主题创建方案
主题设置页 SHALL 允许按 `N` 打开新建弹窗，字段为名称、基于哪套主题、底色、正文色、强调色。三个颜色 MUST 通过循环固定 16 色的方式选择并显示色块预览，MUST NOT 要求用户输入 hex。弹窗打开时 SHOULD 用当前生效的配色预填各字段。

#### Scenario: 从当前配色出发
- **WHEN** 当前主题为「深夜蓝」，用户按 N
- **THEN** 名称有默认值，base 为深夜蓝，三个颜色预填为与其色板最接近的固定色格

#### Scenario: 循环选色
- **WHEN** 用户在某颜色字段按左右键
- **THEN** 选中色格在 16 色中循环，色块、中文色名与 `#rrggbb` 同步更新

#### Scenario: 保存即生效
- **WHEN** 用户填写名称并按 Enter
- **THEN** 新方案写入配置、立即应用，列表「自定义」分组出现该项且处于选中态

### Requirement: 自定义方案覆盖 base 语义色
自定义方案的色板 SHALL 以 `base` 对应主题的完整色板为底，仅覆盖底色、正文色、强调色。未知 base SHALL 回落到默认主题而不是拒绝启动。强调色变化后，强调色上的前景与活动边框 SHALL 一并重算。

#### Scenario: 只改三个颜色
- **WHEN** 某自定义方案基于「米黄纸」，只改了底色与正文色
- **THEN** 选中态、成功、警告等其余字段仍与米黄纸一致，活动边框仍跟随强调色

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
