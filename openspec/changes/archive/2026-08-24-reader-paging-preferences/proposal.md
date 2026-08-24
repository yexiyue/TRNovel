# Proposal: reader-paging-preferences

## Why

issue #63（同一位提出「整页翻页」需求的 Vim 用户）：`PageUp`/`PageDown` 一次整屏平移，翻页后上下屏没有任何公共行，**视觉锚点断裂**——Vim 用户习惯的「保留 N 行上下文」在阅读长文本时缺失。

顺着这条线看阅读页翻页逻辑，还发现三个同源缺陷：

1. **翻页步长硬编码**：`page_lines = height - 3`，用户无法调整，也没有任何「保留上下文」的余地。
2. **`PageUp`/`PageDown` 到章首/章末就卡死**：`ScrollUp`/`ScrollDown` 有完整的「边界武装 → 再按翻章」逻辑，翻页键却只是 `.min(line_count)` 停住。纯用整页翻页读书的用户每到章末都必须切去按 `j` 才能继续——正是提出者的使用方式。
3. **阅读进度坐标系被视口高度污染**：落盘的 `line_percent = current_line / (total_lines - view_height)`，分母含视口高度。换终端、改字号、改窗口大小后，同一个百分比指向不同的行，恢复位置系统性偏移。

同时，TRNovel 已有的可视化设置只覆盖主题（`/theme-setting` 页）和听书（TTS 浮层）；阅读行为偏好（`ReaderDisplayConfig`）至今只有 `show_title` 一项，且只能靠 `v` 键盲切，**没有任何可视入口**。再加一个只能手改 JSON 的翻页参数会让这个缺口更明显。

## What Changes

### 翻页与滚动（`ReadContent`）

- `ReaderDisplayConfig` 新增 `page_overlap: u16`（0..=10，默认 **2**），翻页步长由它派生：`step = (view - overlap).max(1)`。不存储 `step`，随终端高度自适应。
- **末尾留白**：`PageDown` 不再把最后一行强制贴到屏幕底部，允许滚过「贴底位置」让下方自然留白，保证每次翻页步长绝对一致。
- **阅读进度坐标系迁移到绝对比例**：落盘的 `line_percent` 改为 `current_line / total_lines`，与视口高度无关。`1.0` 保留为「章末（贴底）」的语义位置。
- `PageUp`/`PageDown` 与 `ScrollUp`/`ScrollDown` **合并为同一对分支**，只有步长不同（1 vs `step`），边界武装/全书边界提示/翻章逻辑单一实现。
- 底部行号与滚动条改用绝对坐标（`current_line / total_lines`）。

### 阅读设置浮层（新）

- 新增 `ReaderAction::ToggleReaderSettings`，默认绑 `o`。
- 新增 `ReaderSettingsModal`：与 TTS 面板同构的浮层，承载「翻页重叠」（`←/→` 调整，同屏显示派生的实际步长）与「显示标题」两项，均写 `READER_DISPLAY` atom 并落盘 `~/.novel/reader-display.json`。存盘经 debounce，避免连按写穿磁盘。
- 与 TTS 浮层互斥（打开一个自动关另一个），避免两个浮层叠加渲染与键位争抢。
- `SettingItem` 从 `src/pages/read_novel/tts/settings.rs` 上提到 `src/components/`，供两个面板共用。

### 默认键位

- `PageUp`/`PageDown` 默认表补 `ctrl-b` / `ctrl-f`（Vim 整页前后翻），物理 `PgUp`/`PgDn` 保留。

**非目标**：半页翻页（`Ctrl-D`/`Ctrl-U`）作为独立 action；`scrolloff`（光标周围保留行数，本项目无光标概念）；其余页面的键位迁移；把主题设置页升级为通用设置页。

## Capabilities

### New Capabilities

- `reader-paging`: 阅读正文的滚动与翻页模型 —— 视口/总行数/贴底位置/步长的坐标定义、翻页重叠、末尾留白、章内外边界与翻章、进度百分比的绝对坐标语义。
- `reader-settings-panel`: 阅读偏好的可视化设置面板 —— 浮层入口、条目导航与调整、取值范围与派生值展示、防抖落盘、与听书浮层的互斥。

### Modified Capabilities

（无 —— `openspec/specs/` 目前为空，`configurable-keybindings` 归档时未回灌主 specs。本次涉及的键位增量（`toggle_reader_settings` action、`page_up`/`page_down` 默认键位扩充）随所属能力以 ADDED 形式写在上述两个 capability 内。）

## Impact

- **新增代码**：`src/pages/read_novel/settings/`（阅读设置浮层）、`src/components/setting_item.rs`（上提的通用条目组件）。
- **修改代码**：`src/cache/setting.rs`（`ReaderDisplayConfig` + `page_overlap` + 钳位）、`src/keymap/mod.rs`（新 action + 默认键）、`src/pages/read_novel/read_content.rs`（坐标系与翻页逻辑）、`src/pages/read_novel/mod.rs`（浮层挂载与互斥、帮助浮层条目）、`src/pages/read_novel/tts/settings.rs`（`SettingItem` 迁出后 re-export）。
- **持久化**：`~/.novel/reader-display.json` 增字段 `pageOverlap`；旧文件缺字段走 serde default，不报错。
- **兼容性（已确认可破坏）**：`line_percent` 语义变更会让**存量阅读进度一次性漂移**（最多约一屏），换取此后跨终端尺寸稳定。不做存档迁移。
- **默认行为变化**：`page_overlap` 默认 2 而非 0 —— 这是 issue 请求的体验默认值，不追求「与旧版逐帧一致」。
