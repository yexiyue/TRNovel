# Adaptive TTS buffering

## Why

Qwen 首 PCM 很快，但实测完整生成略慢于正常播放。立即播放每个小块可能耗尽队列，产生频繁停顿。

## What Changes

- 核心增加 3 秒预缓冲，耗尽后逐次增加 2 秒恢复目标，上限 10 秒；按播放倍率换算音频时间，继续受 30 秒 / 16 MiB 预算限制。
- 用户暂停与自动缓冲分开；短文本生成结束后直接播放剩余音频，不等待达到目标。
- 协议报告缓冲余量、目标和耗尽次数，TUI 明确显示缓冲；不改变检查点的实际播放语义。
- Qwen 提供可选逐块生成耗时诊断，先测量，不直接修改解码粒度或音频接缝。

## Capabilities

### New Capabilities

- `continuous-playback`: 自适应启动和恢复缓冲。

### Modified Capabilities

无。

## Impact

novel-tts-core、轻量协议、阅读器显示、Qwen 诊断和验收文档。RTF 大于 1 时缓冲不能保证无限连续播放。
