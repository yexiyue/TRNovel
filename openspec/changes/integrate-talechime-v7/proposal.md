# Proposal

## Why

Talechime 已发布协议 v7，阅读器仍使用 v5，无法与新 worker 互通。当前听书设置将日常控制和模型配置混在一起，浏览模型还会中断播放。

## What Changes

- **BREAKING**：升级 talechime-protocol 至 0.3.0 / JSON Lines v7，单音色密封计划覆盖整章，支持流式或整章生成后播放。
- 速度、音量、自动续章立即生效；声音设置持久化为下次播放偏好，不打断当前会话。
- 显式“应用并重播”从最后确认播放完成的原文字节位置恢复，保留暂停状态。
- 听书面板与声音设置分开；模型/音色可搜索选择，风格按能力显示，设备和准备/释放放入高级设置。
- 删除旧强制对齐配置与逐句高亮入口，保留真实片段高亮及跟随。

## Capabilities

### New Capabilities

- `listening-session`: v7 单角色会话、实际播放进度、声音快照和可搜索设置。

### Modified Capabilities

无。阅读设置与听书浮层互斥的既有约束继续适用。

## Impact

Cargo 协议依赖、src/tts、阅读器请求与状态栏、config.toml 的可选 tts.sound 偏好、测试、听书文档和知识库。不引入推理库，不读取 Talechime 数据文件，不实现多角色或整章预生成。
