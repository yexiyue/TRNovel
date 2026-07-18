# Tasks: configurable-keybindings

## 1. 消费端接线（前置：contrib 发布 `ratatui-kit-keymap 0.1.0`）

- [x] 1.1 添加依赖 `ratatui-kit-keymap`（钉版 0.1，启用 `toml` feature；确认无 C 依赖、crossterm 版本统一）
- [x] 1.2 新建 `src/keymap/`（mod.rs 风格）：定义 `ReaderAction`（`#[serde(rename_all = "snake_case")]`），逐键对照 `read_novel` 子树现有 `match KeyCode` 用 builder 声明默认表与 desc（含 `mod.rs` 层目录/TTS 面板/信息浮层入口键）
- [x] 1.3 实现 `~/.novel/keybindings.toml` 加载接线：文件不存在静默用默认；读取/TOML 整体解析失败与 crate 合并告警统一收集（含 tests：无文件、部分覆盖、整文件损坏）
- [x] 1.4 `src/state.rs` 新增 `KEYMAP` Atom；`App` 启动初始化加载并 `KEYMAP.set(...)`，告警接入既有 `WarningModal` 启动错误路径

## 2. 阅读页迁移

- [x] 2.1 `read_content.rs`：事件处理改用 `use_keymap_handler` 按 action 分发，分支体逻辑（`is_scroll`、`Edge` 二次确认、TTS 复位、音量、标题显隐）原样保留
- [x] 2.2 `read_novel/mod.rs` 与 `tts/` 入口键同步迁移到 action 分发
- [x] 2.3 无配置文件下逐键回归：j/k、←/→、PageUp/PageDown、Home/End、+/-、p、v、目录/TTS/信息浮层入口，含章末章首二次确认与全书边界提示
- [x] 2.4 用户覆盖回归：重绑 page_up/page_down 后新键生效、旧键失效、其余键不变；非法配置弹告警且应用可用

## 3. 帮助与提示动态化

- [x] 3.1 `shortcut_info_modal` 阅读页条目改为从 keymap 取键名渲染（未迁移页面保持硬编码）
- [x] 3.2 阅读页底部提示与边界确认提示文案中的键名动态取当前绑定的首个键

## 4. 收尾

- [x] 4.1 用 `to_toml_example()` 生成带注释的完整示例配置（全部 action、默认键、desc），放入 change 目录并用于 issue #49 回复
- [x] 4.2 跑全套 CI 检查（test/clippy/fmt/doc），更新 `dev-notes/knowledge/tui-ratatui-kit.md` 记录 keymap 架构与迁移注意点
