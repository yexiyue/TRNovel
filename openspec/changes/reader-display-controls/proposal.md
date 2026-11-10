# Proposal: reader-display-controls

## Why

阅读页目前只有「翻页重叠 / 显示标题 / 段落间距」三个显示偏好，且没有任何排版参数：

1. **没有版心**：正文永远铺满整个终端宽度。在 2K 显示器上把 Windows Terminal 最大化后，一行能排出七八十个汉字，眼睛要来回横扫，读小说反而比小窗口更累。业内阅读器普遍限制「每行字数」正是这个原因。
2. **没有首行缩进**：中文小说以单换行分段，段落之间没有空行时（默认紧凑排版），段落边界只能靠折行猜，读起来发闷。
3. **边框写死**：正文四周固定单线框，想要沉浸式无边框、或者想要醒目一点的双线框，都只能改代码。
4. **键位不可见**：全部快捷键只有按 `i` 打开帮助页才能看到，新用户上手成本高。

## What Changes

新增一组**纯增量**的阅读显示偏好，全部经「阅读设置」面板（`o`）调整、写入 `config.toml` 的 `[reader]` 段，并在渲染时即时生效：

- `lines_per_row`：正文每行最多显示的字符数（CJK 宽度计），`0` 表示跟随终端宽度。范围 `10..=200`，越界值在加载时钳制。
- `first_line_indent`：段落首行缩进的字符数，`0..=8`；只作用于段首**可视行**，不参与搜索高亮与朗读的字节偏移。
- `paragraph_gap`：段落之间补几个空行，`0..=4`。旧的布尔开关 `paragraph_spacing` 保留为 `paragraph_gap > 0` 的派生视图：读旧配置时 `true` 换算成 1 行，写盘时两字段保持同步。
- `h_margin`：正文左右各留白的列数，`0..=6`。
- `frameline`：`single`/`rounded`/`double`/`thick`/`none`，默认 `single`（与现有外观一致）。`none` 时正文铺满、多出 2 行可用高度。
- `show_frameline_title`：边框上是否显示章节标题。
- `show_hint_bar`：正文下方常显一行快捷键提示，**默认开**。

同时：

- 「阅读设置」浮层从 3 项扩到 9 项（TTS 构建额外 1 项「跟随朗读」，共 10 项），改为双列布局以适配矮终端。
- `visible_lines` 改为由 `ReaderDisplayConfig` 计算 chrome（状态栏 1 行 + 边框 2×inset + 提示条 1 行），三个调用点统一传快照。

## Capabilities

### Added Capabilities

- `reader-display`: 版心、首行缩进、段间距、留白、边框、常显快捷键提示条。

### Modified Capabilities

- `reader-settings-panel`: 条目从 3 项扩为 10 项，改为双列；新增数值型条目的增减语义。

## Impact

- **修改代码**：`src/cache/setting.rs`、`src/pages/read_novel/read_content.rs`、`src/pages/read_novel/search/mod.rs`、`src/pages/read_novel/settings/mod.rs`、`src/pages/read_novel.rs`、`src/keymap.rs`、`src/lib.rs`、`src/pages/theme_setting/mod.rs`。
- **依赖**：新增 `unicode-width`（提示条按显示宽度裁剪，CJK 占 2 列）。
- **兼容性**：新增字段全部 `#[serde(default)]`，旧 `config.toml` 无感；`paragraph_spacing` 双向同步，旧值不会被丢弃。
- **行为变化**：默认开启提示条会让每屏正文少 1 行（可用设置关掉）；需在 README 与 CHANGELOG 写明；字号缩放仍由终端负责（Windows Terminal 的 Ctrl+加号），应用不介入。
