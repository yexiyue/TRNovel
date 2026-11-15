# Design: reader-theme-presets

## D1 保存逐字段覆盖表，而不是整份色板

`CustomTheme` 保存 `name` / `slug` / `base` 与 `colors: BTreeMap<String, String>`（Palette 字段名 → `#rrggbb`）。保存时只记录与 `base` 色板不同的字段，解析时先取 `base` 的完整 `Palette`，再按 `colors` 逐项覆盖。这样调整预设的派生色时，未显式定制的字段会自动继承改进；配置也小得多，用户手改时不容易漏字段。未知字段名与非法 hex 静默忽略并回落 base，避免一个坏键拖垮整套主题。

旧版只有 `bg` / `fg` / `accent` 的方案仍能解析：这三个字段保留在结构体里（序列化时为空则省略），解析时先映射成同名覆盖，再应用 `colors`，因此 `colors` 优先且旧 JSON 不报错。

## D2 解析顺序：自定义 → 预设 → 内置

`AppearanceConfig::palette()` 先查 `custom_themes` 中是否有同 slug 项；命中则调用 `CustomTheme::palette()`，否则维持原来的预设/内置解析。`CustomTheme::palette()` 内部再用 `ThemeChoice::from_slug(base)` 找 base，未知 base 回落 `ThemeName::TokyoNight`。两份回落互不干扰：自定义项自己的 base 坏了只影响该方案的底色来源，不会让整个主题解析失败。

## D3 slug 复用空位而不是「数量 + 1」

`next_slug()` 从 `custom-1` 开始找第一个未被现有自定义项、预设或内置主题占用的编号。不用 `custom-{len+1}` 是因为删掉中间一项后长度会回退，新方案可能撞上仍然存在的旧 slug，导致 `custom_theme(slug)` 取到错误的那一项。

## D4 16 色循环 + 手输 hex 兜底

终端没有系统取色器，纯 hex 输入既慢又无法预览。设置页为每个 Palette 字段提供固定 16 色色格，左右键循环；每行同时显示中文标签、当前色块与使用场景说明，让用户知道这个颜色会出现在正文、页脚还是边框上。颜色列表固定高度、可用 ↑↓ 滚动，15 个字段不会把弹窗撑出屏幕。

固定 16 色覆盖常见浅底/深底/强调色，但无法表达任意品牌色，所以保留手输 `#rrggbb`：输入满 7 位且合法时立即更新当前字段，非法输入不写回。配置仍存 `#rrggbb`，便于手改与跨版本迁移；新建时用 `nearest_custom_color` 把当前主题颜色映射到最近色格。

## D5 派生色只在未显式覆盖时重算

`CustomTheme::palette()` 覆盖 `accent` 后，如果用户没有显式定制 `on_accent`，就重算 `on_accent = readable_foreground(&[accent, selection])`；如果没有显式定制 `border_active`，就让 `border_active = accent`。否则用户换了强调色但焦点框仍是 base 的旧强调色，界面会出现两种互相冲突的「强调色」。

一旦用户在编辑器里明确给 `on_accent` 或 `border_active` 选了颜色，就尊重该值，不再自动派生，避免「我明明选了却被覆盖」。判断依据是 `colors` 表（含旧字段映射）里是否出现对应键。

## D6 分组标题并入第一项而不是独立列表项

`ListSelect` 的每一项都参与键盘选择与鼠标命中。把「── 自定义 ──」做成独立项会让 j/k 在标题上停住、Enter 还会尝试应用一个没有 slug 的假主题。因此标题作为第一个自定义条目的额外一行渲染（该项高度 5，其余 4），列表索引与主题一一对应。

## D7 删除走既有 ConfirmModal，配置层兜底回落

`X` 只为 `ThemeEntry::Custom` 打开确认框，预设/内置主题不可删除。确认后调用 `AppearanceConfig::remove_custom_theme`；该方法在删除的正是当前 `theme_slug` 时把 slug 改回 `DEFAULT_THEME`，因此即使 UI 忘记处理，配置文件里也不会留下指向已删方案的悬空 slug。

## D8 二次编辑复用同一 slug 与同一弹窗

`E` 只在选中项为 `ThemeEntry::Custom` 时进入编辑；`open_custom_editor` 接收可选的 `edit_slug`，命中时预填该方案的名称、`base` 与完整 `Palette`，并把 `editing_slug` 放进草稿。保存时若有 `editing_slug`，就原位替换同 slug 项而不是追加新项；`N` 仍走 `next_slug` 新建。弹窗标题按草稿里的 `editing_slug` 区分「新建方案 / 编辑方案」，其余按键与新建完全一致。
