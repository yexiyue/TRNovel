## Context

TRNovel 当前主题模型如下：

```text
THEME: Atom<ThemeConfig>
  ├─ ThemeColors: text / primary / warning / error / success / info
  ├─ 派生 Style 槽位: basic / search / modal / novel / ...
  └─ novel.show_title
```

这套模型在 `ratatui-kit` 0.7 迁移时很有用：它让项目 wrapper 继续把旧的项目样式映射进框架组件，从而保持行为不变。但现在代价已经显现：约 30 个文件仍然导入 `ThemeConfig`、`UseThemeConfig` 或 `theme.*`，设置页编辑的是底层颜色，阅读标题显示这种偏好也被塞在主题里。

`ratatui-kit` 0.10 已经有原生主题协议：

- `PaletteProvider` 给子树提供 `Palette`。
- 内置组件从最近的 `Palette` 派生自己的 `*Theme`。
- `use_palette` 与 `use_component_theme::<T>()` 读取当前 palette/theme。
- 自定义组件可以实现 `ComponentTheme`。
- 样式 props 是对已解析主题的可选 patch。

`ratatui-kit-themes` 提供命名主题：`ThemeName::all()`、`.slug()`、`.display_name()`、`.next()`、`.prev()`，以及 `terminal_background(palette)`。它 re-export 了 `ratatui-themes`，所以 TRNovel 不应直接依赖 `ratatui-themes`。

关键框架约束：

- `Palette` 是 non-exhaustive，不能用 struct literal 构造；应通过默认值或 `ratatui-kit-themes` 获取。
- 核心组件不会自动消费 `Palette.bg`，应用必须显式设置根背景。
- `use_palette` 是被动读取，不注册 waker；运行时切换主题必须写入驱动 `PaletteProvider` 的响应式状态。
- 内置 `ModalTheme` 是 dim overlay 模式，不从 `Palette.overlay` 派生。

## Goals / Non-Goals

**Goals:**
- 让 ratatui-kit `PaletteProvider` 成为 UI 外观的唯一主题来源。
- 用紧凑、可序列化的外观配置替换 `ThemeConfig`。
- 用 `ratatui-kit-themes` 命名主题替代自定义六色编辑器。
- 删除散落的项目样式槽，让内置组件默认消费框架主题。
- 只在 TRNovel 有领域语义的地方保留小型项目 `ComponentTheme`。
- 将阅读显示偏好从主题状态拆出。
- 接受 breaking 配置变更，不写旧 `theme.json` 迁移逻辑。

**Non-Goals:**
- 保留旧六色主题文件或取色 UI。
- 增加任意自定义 palette 编辑能力。
- 重设计导航、键位、书源行为、TTS 行为，或主题无关的缓存所有权。
- 持久化原始 `Palette` 或内置 `*Theme` 结构。
- 直接增加 `ratatui-themes` 依赖。

## Decisions

### D1: 持久化 `AppearanceConfig`，运行时派生 `Palette`

持久化一个项目自有的小模型：

```rust
pub struct AppearanceConfig {
    pub theme_slug: String,
    pub background: BackgroundMode,
}

pub enum BackgroundMode {
    Theme,
    Terminal,
}
```

`AppearanceConfig::theme_name()` 通过扫描 `ThemeName::all()` 解析 `theme_slug`，未知 slug 回退到默认主题。`AppearanceConfig::palette()` 通过 `IntoKitPalette` 派生 `Palette`；`BackgroundMode::Terminal` 再包一层 `terminal_background`。

**原因**：不序列化外部 enum 内部细节，也不序列化原始 `Palette` 字段；缓存文件小而稳定，上游新增主题字段时也能自然通过转换逻辑流入。

**否决方案**：直接持久化 `Palette`。这会让文件更大，绕开命名主题更新，还要依赖我们并不需要的 serde 支持。

### D2: 用 `APPEARANCE` 替换 `THEME`

使用进程级 atom：

```rust
pub static APPEARANCE: Atom<AppearanceConfig> = Atom::new(AppearanceConfig::default);
```

启动时加载 `appearance.json` 写入 `APPEARANCE`；设置页写入时显式 save 并更新 atom。旧 `ThemeConfig` 缓存路径不再读取。

**原因**：appearance 是 ambient singleton，不依赖 `Drop::save`，符合 `global-state-to-atom` 已经建立的 atom 使用边界。

**否决方案**：保留 `THEME: Atom<ThemeConfig>` 作为兼容适配层。这样会继续保留两套主题体系，重构目标落空。

### D3: `PaletteProvider` 放在应用 shell 边界

`App` 订阅 `APPEARANCE`，派生 palette，然后包裹 router 子树：

```text
PaletteProvider(palette)
  └─ root background layer(style: bg(palette.bg))
       └─ RouterProvider(...)
```

根 `View` 必须显式设置背景，因为 ratatui-kit 核心组件不会自动消费 `Palette.bg`。

**原因**：单一 provider 能保证整个 TUI 共享同一外观。appearance atom 写入会触发 provider 子树重渲染，被动 theme read 也能读到新 palette。

**否决方案**：在各页面单独放 `PaletteProvider`。这会导致路由级样式不一致，也容易让模态或共享组件落在 provider 外。

### D4: 优先使用框架组件主题，只为领域语义定义项目主题

样式消费按这个顺序重构：

1. 内置组件（`Text`、`Border`、`SearchInput`、`Select`、`MultiSelect`、`TreeSelect`、`ConfirmModal`、`AlertModal`、`ShortcutInfoModal`）尽量使用 ratatui-kit 主题默认值。
2. 一次性的语义 span 直接从 `Palette` 派生（`warning`、`error`、`success`、`info`、`fg_dim`）。
3. TRNovel 独有且重复出现的语义用小型 `ComponentTheme`，例如：
   - `AppChromeTheme`：logo、标题、弱提示、元数据 label、空态。
   - `ReaderTheme`：正文、章节标题、底部状态、进度、TTS 高亮。

删除 `UseThemeConfig`。如果保留 helper，它只能返回项目 `ComponentTheme` 或 `Palette`，不能返回旧的派生样式树。

**原因**：大部分样式应该继承框架；项目主题只保留框架无法理解的小说阅读语义。

**否决方案**：新建一个巨大的 `AppTheme` 结构镜像旧 `ThemeConfig`。这只是改名，不是简化。

### D5: 主题设置页改为命名主题选择器

设置页列出 `ThemeName::all()`，用 `.display_name()` 展示、`.slug()` 作为持久化 ID。选择主题后立即保存并更新 `APPEARANCE`。背景模式在 `Theme` 与 `Terminal` 之间切换。重置恢复默认 `AppearanceConfig`。

预览以实用为主：展示关键 palette 字段色板（`fg`、`accent`、`selection`、`success`、`warning`、`error`、`info`、`border`），以及少量示例行，用来覆盖列表高亮、弱文本、模态/搜索语义。

**原因**：命名主题是成套设计且由上游维护。轻量选择器比维护颜色解析器、取色弹窗和手工派生样式矩阵更稳。

**否决方案**：在命名主题之上继续保留手动颜色编辑。这会复杂化持久化和排错；用户直接从预设主题中选择即可。

### D6: 阅读显示偏好不属于主题状态

将 `novel.show_title` 移到独立偏好，例如：

```rust
pub struct ReaderDisplayConfig {
    pub show_title: bool,
}

pub static READER_DISPLAY: Atom<ReaderDisplayConfig> = Atom::new(ReaderDisplayConfig::default);
```

阅读页直接切换并保存该偏好。主题变化不得影响它。

**原因**：标题显示是阅读行为，不是颜色外观。放在主题里会导致主题切换携带无关业务状态。

**否决方案**：把 `show_title` 放进 `AppearanceConfig`。这会继续模糊主题边界。

### D7: 有意不迁移旧缓存

实现不应把旧 `theme.json` 反序列化进新模型。如果只有旧文件，TRNovel 使用默认外观；用户修改外观时写入新缓存。

**原因**：用户已明确选择完全重构且不考虑兼容。删除迁移代码能保持架构干净。

## Risks / Trade-offs

- **[30 个文件的大范围机械迁移]** → Mitigation：尽早删除 `UseThemeConfig` 并频繁编译；用 `rg "ThemeConfig|UseThemeConfig|theme\\." src` 作为收尾门槛。
- **[背景看起来没变]**，因为 `Palette.bg` 不被核心组件自动消费 → Mitigation：根部显式 `View(style: Style::new().bg(palette.bg))`，并手验两种背景模式。
- **[主题切换不刷新被动读取者]**，因为 `use_palette` 不注册 waker → Mitigation：所有读取都放在由 `APPEARANCE` atom 驱动的 `PaletteProvider` 子树下。
- **[缓存 slug 漂移或未知]** → Mitigation：通过 `ThemeName::all()` 解析，未知值回退默认主题，不阻塞启动。
- **[用户旧自定义颜色丢失]** → Mitigation：明确文档化为 breaking，并提供清晰的预设主题选择/重置路径。
- **[项目主题重新膨胀]** → Mitigation：只为重复的领域语义定义 `ComponentTheme`；其他地方优先用内置主题和直接 `Palette`。
- **[阅读标题显示回归]** → Mitigation：任务中单列 `v` 切换、持久化、主题切换不影响该偏好的手验。

## Migration Plan

1. 添加 `ratatui-kit-themes`，创建外观模块：`AppearanceConfig`、`BackgroundMode`、默认主题、slug 解析、palette 派生、load/save、`APPEARANCE`。
2. 添加 `ReaderDisplayConfig` 与 `READER_DISPLAY`，启动期同时加载外观与阅读显示配置。
3. 在 app/router 子树外包 `PaletteProvider`，并显式设置根背景。
4. 删除 `ThemeConfig`、`ThemeColors`、派生样式结构与 `UseThemeConfig`；通过编译错误暴露所有剩余调用点。
5. 重构共享组件，改用 ratatui-kit 内置主题默认值、`use_palette` 或项目 `ComponentTheme`。
6. 重构页面和领域列表项渲染器，去掉 `ThemeConfig` props。
7. 将 `theme_setting` 重写为命名主题/背景模式选择器，删除 `select_color.rs`。
8. 把阅读标题开关从主题状态迁到阅读显示状态。
9. 更新主题用户文档与 `dev-notes/knowledge/tui-ratatui-kit.md`。
10. 运行编译、lint、doc 与 TUI 冒烟验证。

回滚使用普通 git revert。本实现会忽略旧 `theme.json`，因此只要旧文件仍存在，回滚后旧行为仍可恢复；不需要前向迁移。

## Open Questions

暂无。默认外观采用 `TokyoNight` + `Terminal` 背景模式；如果实现和对比度测试发现更适合 TRNovel 的默认主题，再在实现期调整。
