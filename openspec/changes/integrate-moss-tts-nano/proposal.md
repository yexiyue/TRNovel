# Proposal

## Why

听书核心仍将资源准备和程序入口绑定到 Kokoro，难以接入原生流式模型。增加 MOSS-TTS-Nano，提供独立运行、低延迟播放和可复用参考音色，同时明确通用核心和具体模型之间的职责。

## What Changes

- 新增 novel-tts-backends crate，以编译 feature 装配 MOSS 和 Kokoro。
- MOSS 成为默认后端；保留已有配置中指定的后端。
- 原生 Rust CPU ONNX 推理，流式输出 PCM，支持 WAV 音色导入、列出和删除。
- **BREAKING**：升级 JSON Lines 协议，提供后端目录、音色名称及后端切换。
- 核心统一管理流式缓冲、取消、原文进度和播放完成检查点。

## Capabilities

### New Capabilities

- `tts-backends`: 编译时后端装配、原生 MOSS 流式听书、资源校验及音色管理。

### Modified Capabilities

无。听书进程解耦变更尚未归档，其进程隔离约束继续保留。

## Impact

涉及 novel-tts-core、novel-tts-protocol、novel-tts、阅读器听书 UI，以及新增后端 crate。阅读器不链接模型运行时。模型目录新增 ~/.novel-tts/moss；配置路径和检查点格式保留。
