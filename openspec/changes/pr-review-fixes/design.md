# Design: pr-review-fixes

## D1：两个提示开关各管一层

阅读页提示行和设置页的 `Border::bottom_title` 都属于「框底」，由既有 `show_hint_bar` 控制；根布局在 `Outlet` 之后渲染的全局页脚由新增 `show_global_hint` 控制。阅读页不渲染全局页脚，避免同屏两条提示。

这样既保留旧配置键，又让「只想留页脚」或「只想留阅读提示」都能实现，不需要第三个开关。

## D2：翻页留白默认关闭，钳制放在解析之后

`ScrollTarget::Overscroll` 是视口局部的旧版观感，不能删：打开开关后仍要靠它保存「推上去多少」。默认关时，在 `current_line` 解析出贴底位置后再 `min(end_scroll)`，同时 PageDown 分支只有开关打开才写 `Overscroll`。PageUp、滚轮与章末确认逻辑不变。

## D3：标题「可见」与「位置」分离

- `show_title`：标题是否可见。
- `show_frameline_title`：可见时是否放在边框上。
- 无边框时没有边框可放，视为「不放在边框上」。

`inline_title()` 是这条规则的唯一入口；边框标题条件与正文内联标题条件互斥。本地 txt 的章节区间从标题行开始，正文自己就带标题；渲染前比较正文第一个非空行与章节标题，相同则跳过内联标题，单行标题仍在正文最上方。`visible_lines` 的 chrome 加上 `inline_title()` 的 1 行，保证翻页步长、章末与设置页显示的「每页滚动 N 行」一致。内联标题不进入正文排版/搜索字节偏移。

## D4：鼠标命中用固定几何

设置页布局是固定的：边框 1 + 页边距 1，左栏 14 列，内容区从 `2 + 14` 开始。`NAV_WIDTH` / `CONTENT_TOP` / `CONTENT_LEFT` 同时用于渲染和命中判断，避免魔法数漂移。

点击左栏切分组并把光标归零；点击内容行先选中，已选中且属于「阅读」组时按点击位置在行内左/右半决定减/加，与 `AdjustableSettingItem` 的两段式行为一致。滚轮只移动选中项，不直接改值。

## D5：显示顺序与序号解耦

阅读项的实际序号仍是 `describe_setting` / `adjust` 的协议；`SETTING_ROWS` 决定展示顺序。全局设置页也改为遍历 `SETTING_ROWS`，键盘左右键通过 `SETTING_ROWS[cursor]` 找到真实条目，因此追加或重排条目不会改错值。
