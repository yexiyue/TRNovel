# Proposal: global-settings-page

## Why

阅读页的 `o` 浮层只能塞得下 4 个高频排版项；配色、键位、数据目录、构建信息此前散落在主题设置页、帮助浮层和文档里。用户想知道「这个程序把我的数据放哪了」「怎么改键位」「这版带不带网络」时，没有统一的入口。

另外两个阅读手感问题也没有落点：

- 空格是终端里最自然的下翻键，`less`/`man`/`vim` 都认，本阅读器此前只认 PageDown/Ctrl-F。
- 方向键默认逐行滚动，屏幕高的终端里翻一屏要按十几次；但直接改默认会破坏「想微调一行」的习惯，所以需要一个默认关闭的开关。

## What Changes

- 新增 `/settings` 全局设置页，首页菜单「全局设置」进入，左侧 5 个分组、右侧条目：
  - **阅读**：11 个排版/行为项（翻页重叠、显示标题、段间空行、首行缩进、每行字数、左右留白、边框、边框标题、快捷键提示、↑/↓ 翻页、目录占宽 1/4｜1/3｜1/2；TTS 构建多一项「跟随朗读」）。←/→ 直接调值，与阅读浮层共用同一份 `describe_setting` / `adjust`。
  - **外观**：当前主题名、背景模式、顶部栏、退出确认；Enter 进入主题设置页。
  - **键位**：8 个常用动作的实际绑定（随 `keybindings.toml` 变化），并提示配置文件路径。
  - **数据**：配置 / 数据 / 缓存三个目录的绝对路径。
  - **关于**：版本、构建特性（是否含 network、tts）、上游仓库、协议。
- 阅读页把 **Space** 绑到 `page_down`（排在物理键与 Ctrl-F 之后，提示行仍显示 PageDown）。
- `ReaderDisplayConfig` 新增 `arrows_page`（默认 false）：开启后，实际按下的 ↑/↓ 由 `ReadContent` 换成 PageUp/PageDown，`j`/`k` 与自定义键位不受影响，章首/章末的「再按一次」语义一并继承。
- 首页菜单与顶栏「全局设置」按钮改跳 `/settings`；原 `/theme-setting` 保留，标题改为「外观与主题」。

## Capabilities

### Added Capabilities

- `global-settings`: 统一的分组式设置页，覆盖阅读、外观、键位、数据、关于。
- `reader-navigation-keys`: 空格翻页与「↑/↓ 当翻页键」开关。

### Modified Capabilities

- `reader-display-controls`: 新增 `arrows_page` 配置项；阅读浮层改为 4 项的快捷面板，完整列表迁到全局设置页。

## Impact

- **修改代码**：`src/pages/settings/mod.rs`（新增）、`src/pages/mod.rs`、`src/app.rs`、`src/app/navigation.rs`、`src/app/layout.rs`、`src/pages/home.rs`、`src/pages/read_novel.rs`、`src/pages/read_novel/settings/mod.rs`、`src/pages/read_novel/read_content.rs`、`src/keymap.rs`、`src/cache/setting.rs`、`README.md`。
- **依赖**：无新增依赖（`unicode-width` 已在用）。
- **兼容性**：`arrows_page` 带 `#[serde(default)]`，旧配置缺字段按 false 加载；`space` 是新增绑定，用户若在 `keybindings.toml` 里显式重绑 `page_down` 则以其为准。
- **行为变化**：空格从「未绑定」变为翻页；首页「全局设置」进入的不再是主题页。
