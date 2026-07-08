## Why

TRNovel 当前主题层仍是项目私有的 `ThemeConfig` 树：用六个颜色手工派生大量 `Style` 槽位，而 `ratatui-kit` 0.10 已经提供原生的 `PaletteProvider` / `ComponentTheme` 协议，`ratatui-kit-themes` 也提供了现成的命名主题。继续保留两套主题体系，会让主题逻辑重复、`theme.*` 样式读取散落在各组件里，设置页也停留在低层颜色编辑而不是选择一套完整主题。

本 change 明确不兼容旧主题配置，直接围绕最新 ratatui-kit 主题系统重建 TRNovel 的主题切换能力，让架构更小、更贴近框架、更容易维护。

## What Changes

- **BREAKING** 用一个紧凑的外观配置替换 `ThemeConfig` / `ThemeColors` / 派生样式结构，配置只记录命名主题与背景策略。
- **BREAKING** 停止加载、保存和文档化旧的六色 `~/.novel/theme.json` 格式；引入新的外观配置文件，并移除旧格式兼容逻辑。
- 新增 `ratatui-kit-themes` 依赖，通过 `ThemeName` 派生当前 `Palette`，并支持 `terminal_background` 背景策略。
- 在应用根部包裹 `PaletteProvider`，并显式把 `palette.bg` 应用到根 TUI 背景。
- 重构组件与页面，让它们通过 ratatui-kit 内置组件主题、`use_palette` 或项目自定义 `ComponentTheme` 消费主题，而不是读取 `UseThemeConfig` 与散落的 `theme.*` 槽位。
- 将主题设置页重做为命名主题选择器，带预览/色板与背景模式切换；删除手工取色流程。
- 将非主题的阅读显示状态，尤其是阅读页标题显示开关，从主题状态中拆出，避免主题切换携带阅读偏好。
- 更新用户文档和项目知识，记录新的主题模型与依赖使用方式。

## Capabilities

### New Capabilities
- `tui-theme-system`: 覆盖 TRNovel 终端 UI 的主题选择、Palette 提供、持久化与组件主题消费契约。

### Modified Capabilities
<!-- none: openspec/specs/ 当前没有既有 capability，本次是新增主题系统契约。 -->

## Impact

- **依赖**：根 `Cargo.toml` 增加 `ratatui-kit-themes`；不要直接增加 `ratatui-themes`，因为扩展 crate 已经 re-export 它的类型。
- **状态与缓存**：`src/state.rs`、`src/cache/setting.rs`、`src/app/mod.rs` 的启动加载从 `THEME: Atom<ThemeConfig>` 改为外观 atom，并新增独立的阅读显示偏好状态。
- **主题 Provider**：`src/app/mod.rs` 和/或 `src/app/layout.rs` 增加 `PaletteProvider`，并显式应用根背景。
- **Hooks**：`src/hooks/use_theme_token.rs` 删除，或替换为围绕 `use_palette` / `use_component_theme` 的更小帮助函数。
- **组件/页面**：当前约 30 个文件引用 `ThemeConfig`、`UseThemeConfig` 或 `theme.*`；它们需要迁到原生主题槽、项目 `ComponentTheme` 或基于 `Palette` 的局部样式派生。
- **设置 UI**：`src/pages/theme_setting/**` 重写；`select_color.rs` 删除。
- **文档**：`docs/src/content/docs/guides/theme.mdx` 改为描述命名主题与背景模式，不再描述六色手动配置。
- **验证**：运行 `cargo check`、`cargo fmt --all --check`、`cargo clippy --all-targets --all-features --workspace -- -D warnings`，并做 TUI 冒烟验证。重点手验主题切换、阅读标题开关持久化、模态/搜索/列表样式、背景策略。
