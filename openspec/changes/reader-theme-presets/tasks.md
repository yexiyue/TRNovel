# Tasks: reader-theme-presets

## 1. 配置与主题模型

- [x] 1.1 `CustomTheme`（name/slug/base/`colors: BTreeMap<String, String>`）与 `serde(default)`
- [x] 1.2 `parse_hex_color` / `color_to_hex` 严格 `#rrggbb` 往返
- [x] 1.3 `next_slug` 从 `custom-1` 起避开已有自定义、预设与内置 slug
- [x] 1.4 `CustomTheme::palette` 以 base 为底按 `colors` 逐字段覆盖，未知 base 回落默认主题
- [x] 1.5 旧 `bg`/`fg`/`accent` 字段兼容映射，`colors` 优先；未知键与非法色回落 base
- [x] 1.6 `AppearanceConfig.custom_themes` 与解析顺序（自定义 → 预设 → 内置）
- [x] 1.7 `custom_theme` / `remove_custom_theme`（删除当前主题时回落默认）
- [x] 1.8 `ThemeEntry` 统一预设、内置、自定义的列表项

## 2. 固定色格与逐项取色

- [x] 2.1 `CUSTOM_COLORS` 16 色（浅底/深底/强调色）
- [x] 2.2 `nearest_custom_color` 用于从当前主题预填
- [x] 2.3 `PALETTE_FIELDS` 表：15 个 Palette 字段名、中文标签与使用场景
- [x] 2.4 `palette_field` / `palette_field_mut` 按字段名读写 Palette
- [x] 2.5 单测：hex 往返、非法输入、最近色格

## 3. 主题设置页

- [x] 3.1 列表追加「自定义」分组标题，选中项按 slug 定位
- [x] 3.2 `N` 新建弹窗按当前主题预填名称/base/全部颜色；`E` 编辑选中自定义方案并预填其原值
- [x] 3.3 Tab 在名称/基于/颜色列表间切换，↑↓ 滚动颜色项，←→ 取 16 色或输入 `#rrggbb`，Enter 保存，Esc 取消
- [x] 3.4 保存时编辑项覆盖同 slug 同位置，新建项走 `next_slug`；保存后立即应用并选中
- [x] 3.5 `X` 仅对自定义项打开 ConfirmModal，删除后回落并重定位光标
- [x] 3.6 帮助与 ActionBar 补充 N/E/X；弹窗标题区分新建/编辑；重置外观一并清空自定义方案

## 4. 测试与文档

- [x] 4.1 单测：旧 JSON 三色解析并回落、未知字段忽略、逐字段覆盖、非法颜色回落、`colors` 优先、未知 base 回落、slug 不重复
- [x] 4.2 README 增补自定义配色说明
- [ ] 4.3 rustfmt --check / clippy -D warnings / cargo check -j 2
- [ ] 4.4 VHS/截图验收：新建弹窗、色块预览、自定义分组、删除确认
