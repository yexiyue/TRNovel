# Proposal: reader-theme-presets

## Why

上一版已经提供 6 套阅读向预设，但预设只能覆盖多数人的常见偏好：有人想让豆沙绿的底色再深一点，有人希望强调色换成自己习惯的琥珀色。没有自定义入口时，这些需求只能改代码或手动编辑 `config.toml`，而后者要求用户理解 `Palette` 的 15 个字段与 slug 契约。

终端里手敲 hex 体验尤其差：没有取色器、不能预览，输错一位只会得到一片看不清的正文。因此配色自定义以「从现有方案出发 + 有限色格」为主，同时保留手输 `#rrggbb` 作为精确调整的兜底。

## What Changes

- 主题设置页在既有预设与内置主题之后追加「自定义」分组，列出用户创建的方案（中文名），可像普通主题一样选中应用。
- 按 `N` 打开新建弹窗，按 `E` 编辑当前选中的自定义方案：弹窗预填名称、基准主题与全部颜色，保存时覆盖同一个 slug；标题区分「新建方案 / 编辑方案」。
- 弹窗按 Palette 的 15 个字段逐项取色，每行显示中文标签、当前色块与使用场景说明；颜色列表可滚动，`←→` 在固定 16 色里循环，也可直接输入 `#rrggbb`。
- 保存后写入 `[appearance].custom_themes`（新增字段，`serde(default)`），立即 `appearance.set` + `save()`，列表选中该方案。
- 按 `X` 删除当前自定义方案，复用既有 `ConfirmModal` 二次确认；删除正在使用的方案时回落默认主题。
- 配色解析顺序改为：自定义（slug 命中）→ 预设 → 内置主题。自定义方案以 `base` 的色板为底，`colors` 表按 Palette 字段名逐项覆盖；未填字段沿用 base。旧版 `bg`/`fg`/`accent` 字段仍可解析并映射进覆盖流程，`colors` 优先。
- README 增补一小节说明 N/E/X、逐字段取色、颜色列表滚动与持久化位置。

## Capabilities

### Added Capabilities

- `theme-customization`: 从现有主题创建、逐字段调整、二次编辑、应用、删除并持久化用户配色方案。

### Modified Capabilities

- `theme-settings`: 主题列表新增「自定义」分组，新增 N/E/X 快捷键、新建/编辑弹窗与可滚动颜色列表。

## Impact

- **修改代码**：`src/cache/setting.rs`、`src/theme/presets.rs`、`src/pages/theme_setting/mod.rs`、`README.md`。
- **依赖**：无新增依赖；颜色选择使用已在用的 `ratatui::style::Color` 与固定色表。
- **兼容性**：`custom_themes` 带 `#[serde(default)]`，旧配置缺字段照常加载；仅有 `bg`/`fg`/`accent` 的旧方案会映射为颜色覆盖，未知字段名与非法颜色回落 base。自定义 slug 统一为 `custom-<n>`，不与预设/内置主题 slug 冲突。
- **行为变化**：主题设置页多出 N/E/X 三个键位，编辑弹窗从三色改为可滚动的 15 字段取色；重置外观（D）现在也会清空自定义方案，确认文案已写明。
