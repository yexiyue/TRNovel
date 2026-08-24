# Proposal: reader-paragraph-spacing

## Why

PR #66 给阅读正文加了「段落之间保留一个终端空行」的排版，解决了小说正文以单换行分段、直接 `textwrap::fill` 后段落首尾相接、读起来过于紧凑的问题。

但这个行为是**写死的**：`append_wrapped_line` 无条件在每个逻辑段落前插一个空行，用户没有任何开关。这带来两个问题：

1. **偏好不可逆**：段落间距是典型的众口难调项 —— 屏幕小、行距本就宽松的终端里，翻倍的行数意味着一屏能看的正文少了近一半；偏好紧凑排版的用户没有退路。
2. **与既有偏好体系不一致**：`ReaderDisplayConfig` 已经承载 `show_title` 与 `page_overlap`，二者都在「阅读设置」浮层里可视化可调（change `reader-paging-preferences`）。段落间距同属阅读显示偏好，却只能靠改代码，是这套体系里唯一的例外。

## What Changes

- `ReaderDisplayConfig` 新增 `paragraph_spacing: bool`，**默认 `false`**。PR #66 尚未随任何版本发布，故默认关闭等于**已发布行为（v0.16.0）保持不变**；补空行会让正文总行数近乎翻倍、一屏可见内容少近一半，不该替所有人做主。旧配置文件缺字段走 serde default。
- 正文排版（`wrap_content` / `highlight`）按该开关决定是否在段落间补空行。**原文自带的空行始终保留**，不受开关影响 —— 开关管的是「自动补」，不是「过滤原文」。
- 「阅读设置」浮层新增第三个条目「段落间距」，与「显示标题」同构（`←/→` 切换开/关），写 `READER_DISPLAY` atom，走既有防抖落盘路径。面板高度相应加一行。

**非目标**：段落首行缩进；可配置的段间空行行数（>1 行）；行间距（每行之间插空行）；其余页面的排版偏好。

## Capabilities

### Modified Capabilities

- `reader-settings-panel`: 面板条目从两项扩为三项，新增「段落间距」开关。
- `reader-paging`: 正文总行数 `total` 随段落间距变化而变化，滚动坐标系的既有不变量（进度按 `current_line / total` 的绝对比例）在切换时仍须成立。

## Impact

- **修改代码**：`src/cache/setting.rs`（新字段 + 默认值）、`src/pages/read_novel/read_content.rs`（排版按开关走）、`src/pages/read_novel/settings/mod.rs`（第三个条目 + 面板高度）。
- **持久化**：`~/.novel/reader-display.json` 增字段 `paragraphSpacing`；旧文件缺字段走 default，不报错。
- **进度影响**：切换开关会改变正文总行数，当前阅读位置按既有的绝对比例语义重新解析，位置大体保持（取整误差不超过一行级别）。这是用户主动操作且即时可见，不做特殊处理。
- **默认行为**：与已发布的 v0.16.0 一致（段落间距默认关闭）。相对 PR #66 合并后的 main 是一次回退 —— 该行为未随版本发布过，不构成对用户的破坏。
