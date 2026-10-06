# Proposal

## Why

当前 TRNovel 直接持有 Kokoro 模型和音频设备，阅读 UI、合成、配置与播放生命周期相互耦合，基础阅读构建也承担原生音频和推理依赖。将听书移到可独立使用的程序，通过稳定进程协议接入可选 TUI，为轻量阅读发行和后续多后端提供边界。

## What Changes

- 在当前 Cargo workspace 新增独立听书程序，提供文本文件 CLI（状态、暂停、继续、停止、退出）和 JSON Lines 协议模式，共用 `novel-tts-core` 核心。
- TRNovel 编译时选择是否包含听书 TUI；启用后的阅读器也不链接 `novel-tts-core`、Kokoro、ORT 或音频播放库。首轮仅迁移 Kokoro，建立后端接口。
- 阅读器首次使用听书时启动子进程，跨章节复用，退出或主动释放资源时关闭；正文获取、阅读进度和自动续章归阅读器。
- stdin/stdout 传输带版本、请求及会话标识的命令/事件；日志走 stderr，音频在听书进程播放。
- 听书程序独占听书配置的读写职责，保留旧 `~/.novel/tts_config.json` 和模型路径；CLI 与阅读器共用设置。
- 失败停播、用户主动重试，从未完成片段开头恢复；单独保存基于正文摘要和原文字节位置的听书检查点，支持跨启动恢复，不自动开始播放。
- 提供基础阅读包与听书包；后者同目录安装阅读器和听书程序，允许配置程序路径，模型按需下载。
- **BREAKING**：听书核心公开 API 收口到会话、后端与播放事件，移除阅读器对 Kokoro 类型及播放器实现的直接访问；更新库示例及发行说明。

## Capabilities

### New Capabilities

- `tts-process`: 独立 CLI、版本化进程协议、子进程生命周期与程序发现。
- `tts-session`: 合成与播放、错误语义、配置所有权及可恢复听书位置。
- `optional-tts-integration`: 编译时可选听书 UI、阅读器职责、依赖隔离与两种发行包。

### Modified Capabilities

- `reader-settings-panel`: 将听书面板互斥规则限定为启用听书 UI 的构建，基础版保留阅读设置功能。

## Impact

涉及 `crates/novel-tts-core`、新增程序与轻量协议 crate、根 workspace/feature 配置、`src/pages/read_novel`、`src/app`、`src/state.rs`、`src/cache/tts.rs`、`src/keymap`、测试与文档，以及 cargo-dist 元数据、安装器和 ARM64 musl 自定义发布。保留固定 `kokoro-tts 0.3.1` / `ort rc.10` 与动态 MSVC CRT；发布工作流由元数据再生成。阅读进度格式不变，听书检查点使用独立版本化文件。

本变更不接入 MOSS/Qwen、不改善 G2P、不建设运行时 UI 插件/常驻多客户端服务，不加入多角色分析；参考 `dev-notes/tts-backend-plan.md` 的后续调研。
