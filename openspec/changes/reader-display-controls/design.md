# Design: reader-display-controls

## D1 「每行字数」直接作为折行宽度

版心宽度由「终端宽度 − 2×边框 − 2×留白」得到，再与 `lines_per_row` 取小作为 `textwrap` 的宽度。中文按字符数计而不是按列宽计，因为正文以 CJK 为主，字符数与视觉宽度基本一致；混排英文时只是稍紧，不会溢出。

## D2 首行缩进不进 ranges

缩进实现为在段首可视行前置 `Span::raw(" ".repeat(indent))`，不修改原始文本、不改变字节偏移。搜索高亮与 TTS 朗读都按原始字节偏移定位，所以缩进对二者完全透明 —— 若改成在文本里插空格，这两个功能会全部错位。

## D3 paragraph_gap 与旧 paragraph_spacing 的双向兼容

旧字段是布尔值，语义等价于「补 1 行」。保留它作为派生视图：

- `normalize()`：`paragraph_spacing == true` → `paragraph_gap = max(paragraph_gap, 1)`；然后 `paragraph_spacing = paragraph_gap > 0`。
- 面板改动 gap 时同步写两个字段。

这样旧版本写出的配置读进来仍是「有间距」，降级回旧版本也不会变成「无间距」。

## D4 提示条按显示宽度裁剪

提示条内容由 `Keymap<ReaderAction>` 动态生成（重绑键位后提示的就是新键），按 `unicode-width` 累加列宽；放不下整段就丢弃整段，末尾能塞得下才补省略号。不做逐字截断，避免出现「下一」这种半截键名。

后续修订：提示条移到整屏最后一行、边框之外，不再占正文内部行数；`hint_bar` 增加页面参数，阅读页额外并入顶栏的 B 返回 / F10 菜单，其它页只显示当前页可用的全局键。

## D5 visible_lines 单一来源

`visible_lines(height, &ReaderDisplayConfig)` 统一计算 `chrome = 1 + 2*frameline.inset()`（提示条已在整屏最后一行，不再计入）。此前硬编码 `height - 3`，分散在阅读页、设置面板、viewport 计算三处；新增可选边框后必须统一，否则会出现「翻到页尾时最后一行显示不全」的经典 bug。

## D6 设置面板双列

条目从 3 项涨到 10 项，单列会让面板高度超过 24 行的常见终端。改为左右两列各 5 行、面板宽 80%，总高 20 行。键位仍是上下选条目、左右调值，列内循环。
