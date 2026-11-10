# Proposal: reader-theme-presets

## Why

上一版已经提供 6 套阅读向预设，但预设只能覆盖多数人的常见偏好：有人想让豆沙绿的底色再深一点，有人希望强调色换成自己习惯的琥珀色。没有自定义入口时，这些需求只能改代码或手动编辑 `config.toml`，而后者要求用户理解 `Palette` 的 15 个字段与 slug 契约。

终端里手敲 hex 体验尤其差：没有取色器、不能预览，输错一位只会得到一片看不清的正文。因此配色自定义应当以「从现有方案出发 + 有限色格」的方式提供，而不是暴露一个原始文本输入框。

## What Changes

- 主题设置页在既有预设与内置主题之后追加「自定义」分组，列出用户创建的方案（中文名），可像普通主题一样选中应用。
- 按 `N` 打开新建弹窗：名称、基于哪套主题、底色、正文色、强调色。三个颜色都在固定 16 色的循环色格里选择，同时显示色块预览与 `#rrggbb` 值；不让用户手敲 hex。
- 保存后写入 `[appearance].custom_themes`（新增字段，`serde(default)`），立即 `appearance.set` + `save()`，列表选中新方案。
- 按 `X` 删除当前自定义方案，复用既有 `ConfirmModal` 二次确认；删除正在使用的方案时回落默认主题。
- 配色解析顺序改为：自定义（slug 命中）→ 预设 → 内置主题。自定义方案以 `base` 的色板为底，只覆盖 `bg`/`fg`/`accent`，其余字段沿用 base。
- README 增补一小节说明 N/X、16 色循环与持久化位置。

## Capabilities

### Added Capabilities

- `theme-customization`: 从现有主题创建、应用、删除并持久化用户配色方案。

### Modified Capabilities

- `theme-settings`: 主题列表新增「自定义」分组，新增 N/X 快捷键与新建弹窗。

## Impact

- **修改代码**：`src/cache/setting.rs`、`src/theme/presets.rs`、`src/pages/theme_setting/mod.rs`、`README.md`。
- **依赖**：无新增依赖；颜色选择使用已在用的 `ratatui::style::Color` 与固定色表。
- **兼容性**：`custom_themes` 带 `#[serde(default)]`，旧配置缺字段照常加载；自定义 slug 统一为 `custom-<n>`，不与预设/内置主题 slug 冲突。
- **行为变化**：主题设置页多出 N/X 两个键位；重置外观（D）现在也会清空自定义方案，确认文案已写明。
