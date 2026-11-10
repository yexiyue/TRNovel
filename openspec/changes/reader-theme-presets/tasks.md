# Tasks: reader-theme-presets

## 1. 配置与主题模型

- [x] 1.1 `CustomTheme`（name/slug/base/bg/fg/accent）与 `serde(default)`
- [x] 1.2 `parse_hex_color` / `color_to_hex` 严格 `#rrggbb` 往返
- [x] 1.3 `next_slug` 从 `custom-1` 起避开已有自定义、预设与内置 slug
- [x] 1.4 `CustomTheme::palette` 以 base 为底覆盖 bg/fg/accent，未知 base 回落默认主题
- [x] 1.5 `AppearanceConfig.custom_themes` 与解析顺序（自定义 → 预设 → 内置）
- [x] 1.6 `custom_theme` / `remove_custom_theme`（删除当前主题时回落默认）
- [x] 1.7 `ThemeEntry` 统一预设、内置、自定义的列表项

## 2. 固定色格

- [x] 2.1 `CUSTOM_COLORS` 16 色（浅底/深底/强调色）
- [x] 2.2 `nearest_custom_color` 用于从当前主题预填
- [x] 2.3 单测：hex 往返、非法输入、最近色格

## 3. 主题设置页

- [x] 3.1 列表追加「自定义」分组标题，选中项按 slug 定位
- [x] 3.2 `N` 打开新建弹窗，按当前主题预填名称/base/三色
- [x] 3.3 Tab/↑↓ 切字段，←→ 切 base 与 16 色，Enter 保存，Esc 取消
- [x] 3.4 保存后 `save()` + `appearance.set`，列表选中新方案
- [x] 3.5 `X` 仅对自定义项打开 ConfirmModal，删除后回落并重定位光标
- [x] 3.6 帮助与 ActionBar 补充 N/X；重置外观一并清空自定义方案

## 4. 测试与文档

- [x] 4.1 单测：自定义覆盖 base 三色、未知 base 回落、slug 不重复、删除后 TOML 不含该项、旧配置缺字段可加载
- [x] 4.2 README 增补自定义配色说明
- [x] 4.3 `cargo fmt --all` / clippy(`-D warnings`) / check(all-features) / 两个 `--lib` 测试
- [ ] 4.4 VHS/截图验收：新建弹窗、色块预览、自定义分组、删除确认
