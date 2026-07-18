# Proposal: configurable-keybindings

## Why

快捷键目前硬编码在约 22 个文件的 `match KeyCode` 里，用户无法调整（issue #49：笔记本键盘没有 PageUp/PageDown/Home/End，阅读页翻页键既不好用也换不掉）。主题已经可以通过配置文件定制，按键还不行；这是目前呼声最高的可配置性缺口。

## What Changes

- 新增 `src/keymap/` 基础设施：按 scope（第一期为 `reader`）定义语义 action 枚举，默认键在代码内声明，用 `crokey` 解析/格式化键位字符串。
- 新增用户配置文件 `~/.novel/keybindings.toml`（TOML，可写注释；用户只写要覆盖的 action，加载时 merge 到默认之上）。
- 配置加载做冲突/非法校验：非法键位字符串或同 scope 重复绑定时回退默认键，并通过既有 `WarningModal` 提示，不阻断启动。
- 新增 `KEYMAP: Atom<Keymap>`（沿用 global-state-to-atom 模式），页面事件闭包从 `match key.code` 改为按 action 查表分发。
- 第一期迁移阅读页（`ReadContent` / `ReadNovel` / TTS 面板入口键），其余页面保持硬编码，后续变更逐步铺开。
- 阅读页快捷键帮助浮层（`shortcut_info_modal`）与底部提示改为从 keymap 动态取键名显示，保证帮助与实际绑定一致。
- 新增依赖：`crokey`（键位解析/描述）、`toml`（配置反序列化）。

**非目标**（后续变更处理）：shell 键（q/g/b）与列表页迁移、vim 式多键序列（如 `g g`）、docs 站配置文档与 `--export-keymap` 模板导出。

## Capabilities

### New Capabilities

- `keymap-config`: 按键配置的定义、加载与合并 —— action 枚举与默认键、`keybindings.toml` 解析（crokey 键位语法）、部分覆盖合并、冲突/非法校验与回退提示、`KEYMAP` Atom 暴露。
- `reader-keymap`: 阅读页按语义 action 分发按键（滚动/翻页/翻章/TTS/显示切换等），帮助浮层与底部提示从 keymap 动态取键名。

### Modified Capabilities

（无 —— `openspec/specs/` 目前为空，无既存 capability 受影响。）

## Impact

- **新增代码**：`src/keymap/`（action 枚举、Keymap 结构、解析与合并、默认表）。
- **修改代码**：`src/state.rs`（新增 `KEYMAP` Atom）、`src/app/mod.rs`（启动时加载配置 + 校验告警）、`src/pages/read_novel/`（`read_content.rs`、`mod.rs`、`tts/` 入口键）、`src/components/modal/shortcut_info_modal.rs`（阅读页部分动态化）。
- **依赖**：新增 `crokey`、`toml`（均为纯 Rust 小依赖，无 C 依赖影响）。
- **持久化**：新增 `~/.novel/keybindings.toml`（用户手编；程序只读，不走 Drop 自动保存模式）。
- **兼容性**：无配置文件时行为与现状完全一致（默认键即现有键位）；不迁移、不破坏任何既有配置文件。
