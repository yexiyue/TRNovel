# Design: reader-polish

## D1：Esc 交给根布局兜底

根布局（`src/app/layout.rs`）已有 Esc/Backspace 的返回逻辑，问题只在于阅读页把事件吃掉了。
做法是在 `ClearContentSearch` 分支里判断：`query.is_empty() && !editing` 时返回 `EventResult::Ignored`，否则照旧清空并 Consumed。
这样「有搜索 → 清空搜索」与「无搜索 → 返回」两段语义都保留，且不新增键位。

## D2：浮层用「可见行数 + 整页偏移」

`visible_rows(height) = (height - 7) / 3`，下限 1、上限条目数；7 = 面板上下边框 2 + 标题/说明 2 + 底部提示 3。
面板高度 = `rows * 3 + 5`。
`offset = min((cursor / rows) * rows, item_count - rows)`：光标移动时只在跨页时整页翻，避免最后几项出现半屏空白。
渲染用 `element!` 的 `for` 循环逐行生成，复用现有 `setting_row`，不引入滚动容器。

## D3：目录标题按显示宽度硬折

不引入 word-wrap：中文没有空格，按字符累积显示宽度（`UnicodeWidthChar`）折行最稳。
可用宽度 = 目录栏宽度 − 边框 2 − 节点符号 1 − 层级缩进（depth×2）。
`TreeItem` 接收多行 `ratatui::text::Text`，选中框与点击命中自然按整块高度计算。
折叠/展开逻辑不变，折行只发生在渲染阶段。

## D4：留白用内层 View 的 margin

边框的 `padding` 会同时缩进边框内的所有子节点，包括状态栏。
把正文 `Text` 单独包进 `View(height: Fill(1), margin: Margin::new(h_margin, 0))`，边框与状态栏保持原样：留白只影响正文，状态栏回到左右下角。
`visible_lines` 的 chrome 不变（留白是水平方向）。