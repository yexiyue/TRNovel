# Spec: pr-review-fixes

## Requirement: 提示开关

系统 SHALL 提供两个独立的提示开关：

- 关闭「框底提示」时，阅读页底部提示行与设置页边框底部操作说明 MUST 隐藏；全局页脚不受影响。
- 关闭「页底提示」时，非阅读页整屏底部的全局快捷键提示 MUST 隐藏；阅读页提示不受影响。
- 两者默认开启，并持久化到 `config.toml` 的 `[reader]`。

## Requirement: 翻页留白

- 「翻页留白」默认关闭。
- 关闭时，章末最后一页 MUST 停在贴底位置，继续 PageDown MUST NOT 把正文推上去留下空白。
- 打开时，允许恢复旧版 `Overscroll` 观感。
- 该开关 MUST 在阅读设置浮层与全局设置页都能调整并持久化。

## Requirement: 标题可见性与位置

- `show_title = false` 时，任何位置都不显示章节标题。
- `show_title = true` 且标题放在边框上时，正文顶部 MUST NOT 再画一份。
- `show_title = true` 且边框标题关闭或无边框时，章节标题 MUST 显示在正文区域顶部。
- 内联标题 MUST 计入正文可视行数，但 MUST NOT 改变正文排版字节范围或搜索高亮偏移。
- 若正文第一个非空行与章节标题相同（本地 txt 的章节区间从标题行开始），MUST 跳过内联标题，避免顶部出现两行相同标题。

## Requirement: 设置页鼠标

- 点击左栏分组名 MUST 切换分组并把组内光标复位。
- 点击内容行 MUST 选中该条目。
- 对已选中的阅读条目再次点击，左半 MUST 减一档，右半 MUST 加一档。
- 滚轮 MUST 在当前分组内移动选中项，且不得越过边界。
