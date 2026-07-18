# Design: configurable-keybindings

## Context

全项目没有中央 keymap：每个页面/组件在自己的 `use_event_handler` 闭包里 match `KeyCode`（约 22 个文件），键位无法配置（issue #49）。既有可借鉴的项目模式：配置持久化在 `~/.novel/`（`utils::novel_catch_dir()`）；ambient 单例用 module-level `static Atom`（change `global-state-to-atom`）；启动期错误经 `WarningModal` 呈现。事件系统为 ratatui-kit 0.10 的 `use_event_handler(EventScope, EventPriority, |e| -> EventResult)`，输入层（`use_input_layer`）负责文本输入独占，与本设计正交。

主流 TUI（yazi/helix/gitui）的共性：语义 action + 按 UI 上下文分 scope + 代码内默认值 + 用户文件部分覆盖 + 帮助界面从 keymap 动态生成。

## Goals / Non-Goals

**Goals:**

- 用户可通过 `~/.novel/keybindings.toml` 重绑阅读页按键（解决 #49 的核心诉求：无 PageUp/PageDown/Home/End 键的键盘）。
- 无配置文件时行为与现状逐键一致（默认表 = 现有硬编码键位）。
- 帮助浮层/底部提示显示的键名永远与实际绑定一致。
- 基础设施可复用：后续变更把 shell 键与列表页迁进来时只需扩 scope 与 action，不动核心。

**Non-Goals:**

- 不迁移阅读页以外的页面（shell 键 q/g/b、列表页等留给后续变更）。
- 不支持 vim 式多键序列（`g g`）与 Kitty 协议多普通键组合（crokey `Combiner` 不启用）。
- 不提供 TUI 内的按键设置界面（纯配置文件驱动）。
- 不做旧配置迁移（此前不存在按键配置）。

## Decisions

### D1：键位解析用 `crokey`，不用 `keybinds`/`crossterm-keybind`，也不手写

`crokey`（broot 作者维护，v1.4，~410 万下载）提供 `KeyCombination` 的字符串解析（`"ctrl-u"`、`"pagedown"`）、格式化（帮助显示）、`key!` 编译期宏与 serde 支持，恰好覆盖"解析+描述"两个需求且不绑架构。备选：`keybinds`（rhysd，v0.2）带 action 分发器与按键序列，但年轻且序列属非目标；`crossterm-keybind`（v0.4）derive 全家桶，但极新、全局静态初始化风格与 ratatui-kit hooks 模型不合，且与 ratatui 版本 feature 绑定；手写解析则要自担 `F1`/`space`/修饰键别名等长尾。真正的工作量在 22 个文件的语义化重构，解析层选最小最稳的积木。

### D2：配置格式用 TOML（`keybindings.toml`），不沿用 JSON

按键配置是用户**手编**的文件，需要注释与示例；`~/.novel/*.json` 那些是程序写的（Drop 自动保存），性质不同。yazi/helix/television 均用 TOML。新增 `toml` 依赖（纯 Rust）。该文件**程序只读**，不走 Drop 保存模式——避免把用户的注释/格式抹掉。

### D3：action 按 scope 组织，第一期只有 `reader` scope

`Keymap` 结构为 scope → (action → `Vec<KeyCombination>`)。action 枚举按 scope 各自定义（如 `ReaderAction`），避免一个巨型全局枚举；scope 名即 TOML 的表名（`[reader]`）。阅读页 scope 覆盖 `read_novel` 子树当前处理的键：滚动、翻页、显式翻章、Home/End、音量、播放/暂停、标题显隐，以及 `mod.rs` 层的目录/TTS 面板/信息浮层入口键。

### D4：合并语义 = 按 action 整条替换（gitui/crossterm-keybind 模式）

用户写了某 action 就以其键列表**整体替换**默认（写 `page_down = ["ctrl+d"]` 后默认 PageDown 不再生效——用户明确要换键时不留旧键干扰）；没写的 action 保持默认。一个 action 可绑多个键（`Vec`），一个键在同 scope 内只能属于一个 action。

### D5：查表方向为 KeyCombination → Action，加载期构建反查表并校验

加载时把 scope 内所有绑定摊平成 `HashMap<KeyCombination, Action>`；事件闭包里 `KeyEvent` → `KeyCombination::from(key_event)` → 查表得 `Option<Action>` 后 match action，`EventResult` 语义与现状不变（查不到返回 `Ignored`）。校验规则：

- 键位字符串解析失败 → 该 action 回退默认，记一条告警；
- 同 scope 内两个 action 绑了同一键 → 冲突的**用户覆盖**整条回退默认（默认表自身保证无冲突），记一条告警；
- 文件不存在 → 静默用默认；文件存在但整体解析失败（TOML 语法错）→ 全部默认 + 告警。

所有告警在启动后经既有 `WarningModal` 一次性呈现（复用 `App` 启动错误路径），**不阻断启动**。

### D6：`KEYMAP: Atom<Keymap>`，启动加载后只读

沿用 `global-state-to-atom` 模式：`static KEYMAP: Atom<Keymap>`（默认值 = 内置默认表），`App` 的异步初始化里加载配置文件后 `KEYMAP.set(...)`。运行期不写入，无 Drop 存档需求（D2）。页面 `hooks.use_atom(&KEYMAP)` 订阅（注意 `use_atom` 是 `&mut self`，调用点需 `mut hooks`——知识库已记录的坑）。

### D7：帮助与提示从 keymap 取键名

`shortcut_info_modal` 的阅读页条目与阅读页底部提示改为遍历 keymap：action → 键列表 → crokey `KeyCombinationFormat` 渲染成显示字符串。边界提示文案（「再按 ↓ 进入下一章」）中的键名同样动态取绑定的首个键。未迁移页面的帮助条目维持硬编码。

## Risks / Trade-offs

- [crokey 的 shift/大写归一化与现有 `Char('G')` 类匹配存在语义差] → 迁移时逐键对照现有 match 写默认表，并以「无配置文件时逐键行为一致」为验收标准（VHS/手动回归阅读页全部键）。
- [阅读页 `is_scroll` 门控、边界二次确认（`Edge`）、TTS 复位等既有逻辑在重构中被破坏] → 只替换「键 → 分支」的判定层，分支体逻辑原样保留；参照知识库中该文件的已知坑（自借用死锁、边界武装）逐条自查。
- [用户把翻章、滚动绑到同一键等"合法但自伤"的配置] → 同 scope 冲突检测已拦截同键双绑；语义上的怪配置属用户自由，不做价值判断。
- [TOML 手编门槛：用户不知道 action 名与键位语法] → 仓库提供带注释的完整示例配置（列出全部 action、默认键、crokey 语法说明），README/issue 回复中链接；`--export-keymap` 留给后续变更。
- [第一期只覆盖阅读页，用户可能期望全局生效] → proposal 的非目标里明示，issue 回复中说明分期计划。

## Migration Plan

1. 基础设施落地（`src/keymap/` + `KEYMAP` Atom + 加载/校验），不改任何页面——可独立合入，零行为变化。
2. 阅读页迁移到 action 分发 + 帮助/提示动态化，默认表逐键对照现状。
3. 回滚策略：删除配置文件读取即回到内置默认表；默认表与现状逐键一致，故任一阶段回滚均无行为回归。

## Open Questions

- 示例配置放 `docs/` 站还是仓库根 `examples/`？（倾向随后续 docs 变更一起定，本期先放 change 目录附带并在 issue 回复中贴出内容。）
