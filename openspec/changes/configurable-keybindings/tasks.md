# Tasks: configurable-keybindings

## 1. keymap 基础设施

- [ ] 1.1 添加依赖：`crokey`、`toml`（workspace 钉版，确认无 C 依赖引入）
- [ ] 1.2 新建 `src/keymap/`（mod.rs 风格）：定义 `ReaderAction` 枚举与 `Keymap` 结构（scope → action → `Vec<KeyCombination>`），serde 反序列化用 crokey 键位语法
- [ ] 1.3 逐键对照 `read_novel` 子树现有 `match KeyCode` 写出内置默认表（含 `mod.rs` 层目录/TTS 面板/信息浮层入口键），默认表自身做无冲突断言
- [ ] 1.4 实现 `keybindings.toml` 加载与合并：按 action 整条替换；键位解析失败/同 scope 冲突/整文件损坏三类校验，回退默认并收集告警（含 tests：部分覆盖、多键绑定、三类非法配置）
- [ ] 1.5 实现反查接口（`KeyCombination → Option<ReaderAction>`）与键名格式化接口（action → 显示字符串列表，crokey `KeyCombinationFormat`）
- [ ] 1.6 `src/state.rs` 新增 `KEYMAP: Atom<Keymap>`；`App` 启动初始化加载配置并 `KEYMAP.set(...)`，校验告警接入既有 `WarningModal` 启动错误路径

## 2. 阅读页迁移

- [ ] 2.1 `read_content.rs`：事件闭包改为 `KeyEvent → KeyCombination → 查表 → match action`，分支体逻辑（`is_scroll`、`Edge` 二次确认、TTS 复位、音量、标题显隐）原样保留
- [ ] 2.2 `read_novel/mod.rs` 与 `tts/` 入口键同步迁移到 action 分发
- [ ] 2.3 无配置文件下逐键回归：j/k、←/→、PageUp/PageDown、Home/End、+/-、p、v、目录/TTS/信息浮层入口，含章末章首二次确认与全书边界提示
- [ ] 2.4 用户覆盖回归：重绑 page_up/page_down 后新键生效、旧键失效、其余键不变；非法配置弹告警且应用可用

## 3. 帮助与提示动态化

- [ ] 3.1 `shortcut_info_modal` 阅读页条目改为从 keymap 取键名渲染（未迁移页面保持硬编码）
- [ ] 3.2 阅读页底部提示与边界确认提示文案中的键名动态取当前绑定的首个键

## 4. 收尾

- [ ] 4.1 编写带注释的完整示例配置（全部 action、默认键、crokey 语法说明），放入 change 目录并用于 issue #49 回复
- [ ] 4.2 跑全套 CI 检查（test/clippy/fmt/doc），更新 `dev-notes/knowledge/tui-ratatui-kit.md` 记录 keymap 架构与迁移注意点
