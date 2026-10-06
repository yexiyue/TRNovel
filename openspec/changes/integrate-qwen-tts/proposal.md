# Proposal

## Why

听书需要可替换的原生 Rust 模型，以便直接比较 MOSS、Kokoro 和 Qwen 的朗读质量。已有 Candle 实现可以复用，模型依赖应继续隔离在后端层。

## What Changes

- 新增可选 qwen 后端，首版使用固定版本的 Qwen3-TTS 0.6B CustomVoice，提供九种预置音色。
- 复用现有资源校验、下载、会话、PCM 背压、播放和失败检查点机制。
- 按后端报告设备，新增 Candle Metal 选项；自动模式沿用实测校准。MOSS 默认及对齐默认关闭保持不变。
- 明确区分正常 EOS 与帧数超限，不将流结束自动认定为成功。

## Capabilities

### New Capabilities

- `qwen-synthesis`: Candle Qwen 合成、音色、模型资源和设备选择。

### Modified Capabilities

无。

## Impact

novel-tts-backends、worker 的装配和设备选择、轻量协议设备枚举及文档。阅读器仅依赖协议，不引入 Candle。首版提供 CPU/Metal，不宣称 Qwen CUDA 或克隆已可用。
