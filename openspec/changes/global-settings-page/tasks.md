# Tasks: global-settings-page

## 1. 全局设置页

- [x] 1.1 新增 `src/pages/settings/mod.rs`，5 分组（阅读/外观/键位/数据/关于）与 Tab/Shift+Tab 切换
- [x] 1.2 阅读分组复用 `describe_setting` / `adjust`，←/→ 调值
- [x] 1.3 外观分组显示主题名/背景/顶部栏/退出确认，Enter 跳主题设置页
- [x] 1.4 键位分组按 `display_keys` 展示 8 个常用动作与配置文件路径
- [x] 1.5 数据分组显示配置/数据/缓存目录
- [x] 1.6 关于分组显示版本、构建特性、上游仓库、协议
- [x] 1.7 ActionBar 与 `I` 帮助浮层

## 2. 路由与入口

- [x] 2.1 `src/pages/mod.rs` 导出、`src/app.rs` 注册 `/settings` 路由
- [x] 2.2 首页菜单与顶栏按钮改跳 `/settings`；`/theme-setting` 标题改为「外观与主题」
- [x] 2.3 `src/pages/read_novel.rs` 的 `mod settings` 提升为 `pub(crate)`

## 3. 阅读手感

- [x] 3.1 `space` 绑定到 `page_down`（排在物理键与 Ctrl-F 之后）
- [x] 3.2 `ReaderDisplayConfig.arrows_page`（`serde(default)`，默认 false）
- [x] 3.3 `ReadContent` 在 `arrows_page` 开启时把实际按下的 ↑/↓ 翻译为 PageUp/PageDown
- [x] 3.4 设置条目 9「↑/↓ 翻页」，浮层与设置页同步

## 4. 测试与文档

- [x] 4.1 单测：`arrows_page_is_opt_in_and_round_trips`、keymap 空格绑定与首键显示
- [x] 4.2 设置页 `every_section_has_rows`、`item_line_fills_the_whole_width`
- [x] 4.3 README 增补全局设置页与空格/方向键翻页
- [x] 4.4 `cargo fmt --all --check`、clippy(`-D warnings`，全特性与纯本地)、`cargo test --lib`
- [x] 4.5 截图验收：设置页五个分组、空格翻页、方向键翻页开关（本机用 PTY 文本截图逐屏核对，未用 VHS）
