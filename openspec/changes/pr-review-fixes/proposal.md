# Proposal: pr-review-fixes

## Why

PR #82 的维护者体验后提出四个问题，前三个属于新增设置页与旧行为的衔接，第四个是标题开关的语义冲突：

1. 全局设置页只能用键盘，鼠标不能切分组、选条目或调值，与其它页面不一致。
2. 底部提示散落在阅读页、设置页边框和全局页脚，却没有统一开关；维护者希望简洁界面时无法一次关掉。
3. 翻到章末最后不足一页时继续翻页会把正文推上去，下面留一块空白；维护者希望最后一页贴底铺满。
4. 「边框标题」关掉后，即使「显示标题」开着，章节标题也完全消失；标题的**可见性**与**位置**被混在同一个开关里。

用户补充：框底提示与页底提示要各自独立；翻页留白要做成独立开关且默认关闭，需要旧观感的用户可自行打开。

## What Changes

- 全局设置页增加鼠标支持：点左栏切换分组，点内容行选中条目；已选中的阅读项再点一次，左半减、右半加。滚轮移动当前条目。
- 提示拆成两个配置项：
  - `show_hint_bar`（框底提示）：阅读页底部提示行 + 设置页边框底部的操作说明；沿用旧字段，默认开。
  - `show_global_hint`（页底提示）：非阅读页整屏最底部的全局快捷键提示；新增，默认开。
- 新增 `allow_page_blank`（翻页留白），默认 `false`。关闭时章末最后一页钳在贴底位置，不再产生 `ScrollTarget::Overscroll`；打开时恢复旧行为。
- 新增 `ReaderDisplayConfig::inline_title()`：`show_title && (frameline == None || !show_frameline_title)`。满足时章节标题画在正文区域顶部；边框 `top_title` 仍只在标题放边框上时出现，不会重复。
- 阅读页 `visible_lines` 为内联标题预留 1 行，翻页步长与章末判定同步。

## Capabilities

### Added Capabilities

- `settings-mouse`: 全局设置页的鼠标分组、选择与调值。
- `hint-visibility`: 框底提示与页底提示的独立开关。

### Modified Capabilities

- `reader-display`: 标题可见性与位置分离；新增翻页留白开关。
- `reader-pagination`: 默认最后一页贴底。

## Impact

- **修改代码**：`src/cache/setting.rs`、`src/config.rs`、`src/app/layout.rs`、`src/pages/settings/mod.rs`、`src/pages/read_novel/settings/mod.rs`、`src/pages/read_novel/read_content.rs`、`README.md`。
- **配置兼容**：新字段都有 `serde(default)`，旧 `config.toml` 可继续读取；`show_hint_bar` 语义扩大到设置页边框说明。
- **行为变化**：翻页留白默认由「开」变「关」；标题在无边框/关边框标题时不再消失。
