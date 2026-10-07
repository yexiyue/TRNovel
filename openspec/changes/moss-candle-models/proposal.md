# Proposal

## Why

用户实际试听发现 OmniVoice 句中漏字，听感不如 ZipVoice FP32，而 VoxCPM2 最好。需要评估 MOSS 的其他模式，使用工作区统一 Candle 扩展 GPU 听书选择，而不是把现有 Nano 的 ONNX 权重目录直接替换。

## What Changes

- 增加独立的 MOSS Candle 推理库，共用工作区 Candle 0.9.2 和平台 feature。
- 顺序验证 Local 1.7B、Realtime 1.7B 和 VoiceGenerator 音色设计；只发布通过真实推理验证的模式。
- 复用模型目录、音色记录、下载完整性、有界 PCM、取消和配置机制，保留 Nano 和已有用户选择。
- 固定源码及权重 revision，记录中文完整性、性能、显存、取消和试听证据；用户反馈作为未完成音质验收记录。

## Capabilities

### New Capabilities

- `moss-candle-models`: 原生 Candle MOSS 模型推理及验证后开放的模型与音色模式。

### Modified Capabilities

无。

## Impact

新增 `crates/moss-tts`，修改后端 registry 与 worker feature/音色管理。阅读器继续仅依赖协议，模型缓存使用 `.novel-tts/moss/models/<id>/<revision>`，旧 Nano 格式不变。Python 仅允许开发时对照官方实现，不进入发布运行。
