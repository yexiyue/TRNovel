# 顶部标签设置页验收

日期：2026-11-15。基线：`c04fd5b`。方案与任务见 `openspec/changes/settings-tabs-and-controls/`。

## 交互约定

- 五个顶部标签代替侧栏，表单最大 88 列；阅读按排版、翻页、界面和听书分组。
- Tab/Shift+Tab 切换标签栏与表单焦点；标签栏左右切页，表单上下选项、左右调值，Space/Enter 激活。Ctrl+PageUp/PageDown 可跨焦点区切换标签。
- Hover 只改变视觉反馈，不切页、不移动键盘焦点、不改值；标签、控件和选择器均有反馈。
- 开关单击切换，数值仅通过可见 [-]/[+] 调整且边界禁用，枚举打开独占选择器。点击设置项标签只聚焦，取消旧的隐形左右半行操作。
- 滚轮与滚动条自由移动视口；重新进入表单或恢复键盘操作后显露当前项，避免调整屏外设置。枚举支持键盘、鼠标、Esc 和外点取消；外点不会穿透操作背景控件。
- 阅读配置仍防抖保存并在卸载时补写，外观沿用 ConfigStore 保存。提示开关、配色和配置格式保持既有约定。

方案参考：[Fluent Tabs](https://fluent2.microsoft.design/components/web/react/core/tablist/usage)、[WAI Tabs](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/)、[GNOME 设置行](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.8/boxed-lists.html) 和 [Textual Tabs](https://textual.textualize.io/widgets/tabs/)。这是面向终端的适配，没有引入 Web UI 框架。

## 自动检查

- 全功能 locked workspace lib/tests/examples：319 项通过。
- 纯本地 lib：144 项通过；纯本地 + TTS lib：168 项通过。
- 全目标全功能 workspace Clippy（`-D warnings`）、格式检查、严格 rustdoc、全功能及无默认 feature 的主程序构建通过。
- `openspec validate settings-tabs-and-controls --strict` 与 `git diff --check` 通过。
- 回归覆盖共享设置项完整性、共享边界规则、控件绘制与命中、极窄/极矮尺寸、独立 hover/键盘焦点/当前标签以及视口显露。

## 可重跑的真实终端回归

macOS/Linux（POSIX PTY）：

```sh
cargo build --locked --all-features --bin trn
uv run tests/pty/settings_tabs.py
```

脚本使用独立临时用户目录；可用 `TRNOVEL_TEST_BINARY` 指定程序路径，`TRNOVEL_TEST_ARTIFACTS` 指定输出目录。它向真实程序发送键盘及 SGR 鼠标事件，并断言终端画面、单元格样式和 config.toml：

1. 首次进入即有标签 hover，悬停不切页，键盘仍操作原焦点。
2. 滚轮移走当前项后，Tab 回到表单会显露当前项；表单内继续键盘调整也会显露它。
3. 滚轮、滚动条拖动不改值和焦点，控件左右按钮无需二次点击。
4. 提示开关隐藏/恢复操作说明，焦点仍可用。
5. 枚举键鼠选择、Esc 取消；点击浮层外的背景加号只关闭浮层，不改配置。
6. 末项可达；阅读与外观开关、背景模式均可直接操作并落盘。
7. 主题页和设置页返回保留分组与条目位置，所有标签可鼠标访问。
8. 52×18 窗口可访问首尾控件，空白点击不改值；退出前不足防抖间隔的最后一次调值正常保存。

复查指出的焦点显露遗漏已修复，并加入上述第 2 项回归。

下图来自本次真实 PTY 字符网格与颜色导出，不是设计稿：

本轮不包含 Windows 实机或真实 Talechime 音频验收。

截图仅保留在本地 `dev-notes/images/`（Git 忽略），不随仓库上传；文字验收记录和复现脚本继续入库。
