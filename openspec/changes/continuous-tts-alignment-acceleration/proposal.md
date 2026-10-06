# Proposal

## Why

短句独立合成造成顿挫，合成块与高亮共用边界限制了自然朗读。需要保留首音频速度，同时获得实际音频对应的句子进度，并在 CPU / CoreML / CUDA 中选择有效率的设备。

## What Changes

- 合成文本映射、时长预算、首尾静音控制及连续播放标记。
- 独立原生 Qwen ONNX 对齐器；异步逐句高亮与可靠降级。
- **BREAKING** 协议 v3 和设备选择配置。
- 可选 CoreML / CUDA、端到端校准、模型与运行库资源记录。

## Capabilities

### New Capabilities

- `listening-timeline`: 合成块、句子时间线、播放位置及检查点。
- `inference-devices`: 设备选择、校准、对齐模型资源和失败处理。

### Modified Capabilities

无。

## Impact

novel-tts-core、novel-tts-backends、novel-tts、novel-tts-protocol、阅读器 TUI、构建及发布文档。
