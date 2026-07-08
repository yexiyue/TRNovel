## 1. 外观模型与全局状态

- [x] 1.1 在根 `Cargo.toml` 增加 `ratatui-kit-themes` 依赖，并确认没有直接增加 `ratatui-themes`
- [x] 1.2 新增或重构外观配置模型：`AppearanceConfig`、`BackgroundMode`、默认主题 `tokyo-night`、默认背景模式 `Terminal`
- [x] 1.3 为 `AppearanceConfig` 实现 `theme_slug` 解析、`ThemeName::all()` 回退查找、`Palette` 派生、`terminal_background` 应用
- [x] 1.4 为外观配置实现新缓存文件的 load/save，且不读取旧 `theme.json`
- [x] 1.5 新增 `ReaderDisplayConfig`，包含 `show_title` 默认值、load/save 与独立缓存文件
- [x] 1.6 在 `src/state.rs` 中用 `APPEARANCE: Atom<AppearanceConfig>` 和 `READER_DISPLAY: Atom<ReaderDisplayConfig>` 替换 `THEME: Atom<ThemeConfig>`
- [x] 1.7 更新 `src/app/mod.rs` 启动加载逻辑，加载外观与阅读显示偏好，并保持旧 `theme.json` 存在时不影响启动

## 2. PaletteProvider 根接入

- [x] 2.1 在 `App` 中订阅 `APPEARANCE`，每帧从当前外观派生 active `Palette`
- [x] 2.2 用 `PaletteProvider` 包裹 router/provider 子树，保证模态、页面、共享组件都在同一主题上下文内
- [x] 2.3 在 app shell 根 `View` 或 `Layout` 根节点显式应用 `Style::new().bg(palette.bg)`，覆盖 `Theme` 背景模式
- [x] 2.4 手动切换 `BackgroundMode::Terminal` 时确认根背景保留终端背景，其他 palette 字段仍生效

## 3. 项目主题原语与共享组件

- [x] 3.1 新增项目主题模块，定义小型 `ComponentTheme`，至少覆盖 `AppChromeTheme` 与 `ReaderTheme`
- [x] 3.2 删除 `src/hooks/use_theme_token.rs` 与 `UseThemeConfig` 导出，必要 helper 只返回 `Palette` 或项目 `ComponentTheme`
- [x] 3.3 重构 `Loading`、`SearchInput`、`ConfirmModal`、`WarningModal`、`BrowserPromptModal`、`ShortcutInfoModal`，使用内置组件主题、`use_palette` 或项目 `ComponentTheme`
- [x] 3.4 重构 `Select`、`ListSelect`、`MultiListSelect`、`FileSelect`，保留现有 loading、空态、虚拟化和目录过滤行为，同时去掉 `ThemeConfig` 依赖
- [x] 3.5 移除共享组件中不必要的显式样式覆盖，让 ratatui-kit 内置主题默认值承担基础文本、边框、高亮和语义状态

## 4. 页面与领域渲染器迁移

- [x] 4.1 重构 `Home`、`SelectHistory`、`SelectFile`、`BookDetail`、`BookSourceLogin`、`SelectBooks` 等页面，改用 palette/theme 派生样式
- [x] 4.2 重构网络搜索结果、书源选择、导入书源、历史记录等自定义 list item，删除结构体中的 `ThemeConfig` 字段
- [x] 4.3 重构阅读页正文、章节目录、TTS 设置、TTS 下载和声音选择，使用 `ReaderTheme` 或当前 palette
- [x] 4.4 将阅读页 `v` 键标题显示切换改写到 `READER_DISPLAY`，保存 `ReaderDisplayConfig`，并保证主题切换不改变该偏好
- [x] 4.5 运行 `rg "ThemeConfig|ThemeColors|UseThemeConfig" src`，确认无活跃引用
- [x] 4.6 运行 `rg "theme\\." src`，确认无旧派生样式槽访问残留；允许非主题语义命名的误报需逐项核对

## 5. 主题设置页重写

- [x] 5.1 删除 `src/pages/theme_setting/select_color.rs` 与旧取色弹窗流程
- [x] 5.2 将主题设置页改为基于 `ThemeName::all()` 的命名主题列表，显示 `display_name()` 并用 `slug()` 保存
- [x] 5.3 实现主题选择后立即保存 `AppearanceConfig` 并更新 `APPEARANCE`
- [x] 5.4 实现 `Theme` / `Terminal` 背景模式切换，并立即保存与应用
- [x] 5.5 实现 palette 预览区域，展示关键色板和列表/弱文本/语义状态示例
- [x] 5.6 实现重置确认逻辑，恢复默认主题与默认背景模式
- [x] 5.7 更新主题设置页快捷键说明，删除旧的六色取色说明

## 6. 文档、项目知识与验证

- [x] 6.1 用中文更新 `docs/src/content/docs/guides/theme.mdx`，描述命名主题、背景模式、重置行为和旧六色编辑的 breaking removal
- [x] 6.2 更新 `dev-notes/knowledge/tui-ratatui-kit.md`，记录 `PaletteProvider`、`ratatui-kit-themes`、`Palette.bg` 需根部显式应用、`use_palette` 被动读取等项目结论
- [x] 6.3 确认旧 `~/.novel/theme.json` 只被清理命令间接删除，不再被启动加载或新主题系统读取
- [x] 6.4 运行 `cargo check`
- [x] 6.5 运行 `cargo fmt --all --check`
- [x] 6.6 运行 `cargo clippy --all-targets --all-features --workspace -- -D warnings`
- [x] 6.7 运行文档检查命令或等效验证，确保主题指南不含旧六色流程
- [ ] 6.8 做 TUI 冒烟验证：主题切换、背景模式、重置、搜索框、列表选中、模态、阅读标题 `v` 切换与重启后持久化
