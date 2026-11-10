# Design: reader-theme-presets

## D1 自定义方案存语义色，不存整份色板

`CustomTheme` 只保存 `name` / `slug` / `base` / `bg` / `fg` / `accent`，解析时先取 `base` 的完整 `Palette`，再覆盖三个语义色。这样以后调整预设的派生色（例如选中态对比度）时，自定义方案会自动继承改进，而不是被冻结在一份旧快照里；配置也小得多，用户手改时不容易漏字段。

## D2 解析顺序：自定义 → 预设 → 内置

`AppearanceConfig::palette()` 先查 `custom_themes` 中是否有同 slug 项；命中则调用 `CustomTheme::palette()`，否则维持原来的预设/内置解析。`CustomTheme::palette()` 内部再用 `ThemeChoice::from_slug(base)` 找 base，未知 base 回落 `ThemeName::TokyoNight`。两份回落互不干扰：自定义项自己的 base 坏了只影响该方案的底色来源，不会让整个主题解析失败。

## D3 slug 复用空位而不是「数量 + 1」

`next_slug()` 从 `custom-1` 开始找第一个未被现有自定义项、预设或内置主题占用的编号。不用 `custom-{len+1}` 是因为删掉中间一项后长度会回退，新方案可能撞上仍然存在的旧 slug，导致 `custom_theme(slug)` 取到错误的那一项。

## D4 固定 16 色而非 hex 输入

终端没有系统取色器，手敲 hex 既慢又无法预览。设置页提供 16 个覆盖浅底/深底/强调色的色格，左右键循环；每个字段同时显示色块、中文色名与最终 `#rrggbb`，让用户知道配置里会写成什么。配置仍存 `#rrggbb`，便于手改与跨版本迁移。新建时用 `nearest_custom_color` 把当前主题的 bg/fg/accent 映射到最近色格，实现「从现有配色出发」。

## D5 只覆盖三个语义色，派生色跟着重算

`CustomTheme::palette()` 覆盖 `accent` 后同步重算 `on_accent = readable_foreground(&[accent, selection])` 与 `border_active = accent`。如果不重算，用户换了强调色但焦点框仍是 base 的旧强调色，界面会出现两种互相冲突的「强调色」。

## D6 分组标题并入第一项而不是独立列表项

`ListSelect` 的每一项都参与键盘选择与鼠标命中。把「── 自定义 ──」做成独立项会让 j/k 在标题上停住、Enter 还会尝试应用一个没有 slug 的假主题。因此标题作为第一个自定义条目的额外一行渲染（该项高度 5，其余 4），列表索引与主题一一对应。

## D7 删除走既有 ConfirmModal，配置层兜底回落

`X` 只为 `ThemeEntry::Custom` 打开确认框，预设/内置主题不可删除。确认后调用 `AppearanceConfig::remove_custom_theme`；该方法在删除的正是当前 `theme_slug` 时把 slug 改回 `DEFAULT_THEME`，因此即使 UI 忘记处理，配置文件里也不会留下指向已删方案的悬空 slug。
