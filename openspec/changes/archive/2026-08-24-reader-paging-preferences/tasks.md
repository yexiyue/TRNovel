# Tasks: reader-paging-preferences

## 1. 配置层

- [x] 1.1 `src/cache/setting.rs`：`ReaderDisplayConfig` 增 `page_overlap: u16`（`#[serde(default)]`，默认 2），加 `page_overlap()` 访问器在类型内钳到 `0..=10`，加 `PAGE_OVERLAP_MAX` 常量
- [x] 1.2 单测：默认值、旧 JSON 缺字段走默认、越界值被钳位、`show_title` 不受影响

## 2. 滚动坐标系（`read_content.rs`）

- [x] 2.1 把 `line_count`（实为 max_scroll）拆成 `total` / `view` / `end_scroll` 三个量，并按 design §1.2 重写 `current_line` 的 memo：`percent >= 1.0 → end_scroll`，否则 `round(percent * total).min(total - 1)`
- [x] 2.2 所有写回点改为 `line_percent = current_line / total`（`total == 0` 时为 0）
- [x] 2.3 底部行号分母改 `total`；`use_scrollbar` 的 content_length 改 `total`
- [x] 2.4 `step` 由 `view` 与 `page_overlap()` 派生（`(view - overlap).max(1)`），替换硬编码 `page_lines`

## 3. 翻页与边界合并

- [x] 3.1 `ScrollDown | PageDown` 合并为单一分支，`delta` 取 1 或 `step`；边界分支（`AtLast` / `Edge::Next` 武装 / 翻章）单一实现
- [x] 3.2 `ScrollUp | PageUp` 同构合并，章首武装后再按落到上一章章末
- [x] 3.3 验证末尾留白不越界（design §1.3 的上界推导），`GoBottom` 仍落贴底

## 4. 键位

- [x] 4.1 `src/keymap/mod.rs`：`ReaderAction` 新增 `ToggleReaderSettings`，默认绑 `o`，desc「打开/关闭阅读设置」
- [x] 4.2 `page_up` 默认键补 `ctrl-b`、`page_down` 补 `ctrl-f`（物理键保留在首位，保证提示仍显示 PageUp/PageDown）
- [x] 4.3 `src/keymap/tests.rs` 补测：新 action 可被 `o` 命中、`ctrl-f` 命中 `PageDown`、用户覆盖后默认键失效

## 5. 阅读设置浮层

- [x] 5.1 `SettingItem` 从 `pages/read_novel/tts/settings.rs` 上提到 `src/components/setting_item.rs`，TTS 侧改为 re-export，调用点不变
- [x] 5.2 新建 `src/pages/read_novel/settings/mod.rs`：`ReaderSettingsModal`（`Modal` + `ScrollView` + `index` 导航，`blocks_lower: false`），两个条目组件（翻页重叠 / 显示标题），逐项 `is_editing` 门控
- [x] 5.3 翻页重叠条目同屏显示派生步长（需要 `view`，由 props 下传终端高度）
- [x] 5.4 用 `use_debounce_effect` 以配置值为 deps 防抖落盘，替换 `ToggleTitle` 现有的即时 `save()`
- [x] 5.5 `read_novel/mod.rs`：新增 `settings_open` state、`ToggleReaderSettings` 分支（与 TTS 互斥）、挂载浮层、`is_scroll` 门控加入新浮层

## 6. 帮助与提示

- [x] 6.1 快捷键帮助浮层阅读模式条目加入「打开/关闭阅读设置」，键名走 `display_keys`
- [x] 6.2 阅读设置浮层开启时的帮助条目（上一项/下一项/调整/关闭）

## 7. 验收与收尾

- [x] 7.1 手工回归（VHS 实测）：重叠 0/2/6 三档下的翻页步长（28/26/22 行，逐档核对）、末尾留白（104/121 仍显示最后一行 + 11 行留白）、逐行滚动仍贴底（93/121）、章末章首二次确认（滚动键与翻页键各一遍）、全书边界提示「已是全书最后一章」
- [x] 7.2 手工回归：进度跨终端尺寸恢复（改窗口高度后重进同章位置一致）、留白位置往返稳定、`End`/顶部翻回上一章落贴底
- [x] 7.3 全套 CI 检查（test / clippy -D warnings / fmt / doc）
- [x] 7.4 更新 `dev-notes/knowledge/tui-ratatui-kit.md`：滚动坐标系约定、`percent = 1.0` 语义、浮层互斥与 `blocks_lower` 复检
- [x] 7.5 更新 docs 站：`read.mdx` 新增「阅读设置」章节与章末二次确认说明，`keybindings.mdx` 补 `ctrl-b`/`ctrl-f` 默认键与 `toggle_reader_settings`
- [x] 7.6 `/simplify` 四角度审查（复用/简化/效率/抽象层次）并应用修复：`ScrollTarget` 取代 `percent>=1.0` 哨兵、`Panel` enum 取代两个 bool、`AdjustableSettingItem` 上提按键协议（5 个调用点退化）、步进与钳位收进 `ReaderDisplayConfig`、帮助表与关闭态浮层不再每帧构建、防抖落盘去掉挂载帧空写并移出渲染线程
- [x] 7.7 重构后 VHS 全量复验（翻页步长/末尾留白/章末武装/面板调整/TTS 面板渲染均与重构前一致）
- [x] 7.8 `/code-review` 三角度正确性审查（滚动坐标系 / 状态持久化 / 事件等价性）并修复：`Ratio` 解析钳到 `end_scroll` + 新增 `Overscroll` 变体承载视口局部留白、`on_select` 补重置、滚动条 content_length 改回 `end_scroll`、行号分子改最后可见行、防抖落盘加 `use_on_drop` 兜底与失败重试、shell 键排除 Ctrl/Alt（`ctrl-b` 曾被当成「返回」）、`scroll_to_index` deps 补终端高度
- [x] 7.9 补 5 项坐标系不变量单测（视口下限、比例往返不撞 `ChapterEnd`、跨视口钳位、留白仅存活于本视口、空内容安全）
- [x] 7.10 回复 issue #63：v0.16.0 已发布并在 issue 下说明「阅读设置」面板与末尾留白行为，提出者确认后关闭
