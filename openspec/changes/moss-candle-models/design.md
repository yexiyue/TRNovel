# Design

## Context

现有 MOSS 是 Nano 的固定 ONNX 图与 SentencePiece；Qwen 和 Omni 已统一 Candle 0.9.2。Local 1.7B 使用 Qwen3 全局骨干与无位置编码的局部深度 Transformer，Realtime 是另一种分层文本音频架构，VoiceGenerator 使用 delay pattern。三者使用独立的 MOSS Audio Tokenizer，不能复用 Nano 或 Qwen codec 权重。

## Goals / Non-Goals

Goals: 原生 Rust/Candle 推理与 GPU 验证，保持进程隔离、设备显式错误、取消和资源完整性。
Non-Goals: 改变用户默认选择、发布依赖 Python、将未验收模式伪装成可用模型、以 EOS 替代文字完整性验收。

## Decisions

- 使用一个 `moss-tts` 计算库，配置驱动共享 Transformer、采样和 MOSS codec。Local、Realtime、Delay 的调度分别实现，避免复制骨干计算。
- 固定官方源码 revision `934d6826b084c46a0d033402174d5f8ac4ed2519`；各权重和 codec 固定自己的 commit、尺寸与 SHA-256。
- 优先 Local 1.7B text-only 真生成，再实现参考编码、渐进 PCM 和取消；Realtime 和设计作为后续同库模式。原始 token/WAV 与官方对照留存。
- 新模式的模型文件放用户默认缓存；在真实推理通过之前只存在探针，不加入 registry，Nano 路径和记录格式不变。
- 后端在加载 await 前持有推理线程所有权；关闭音频 receiver 停止推理，释放旧模型后加载新模型。

## Risks / Trade-offs

- Codec 本身约 1.77B，显存需求不能只按文本骨干估算；CUDA 模型使用官方 BF16、codec F16，CPU 数值验证使用 F32。VoiceGenerator 真实 F16 溢出已验证，不能把 CPU 检查视为 GPU 精度验收。
- Candle 缺少上游专用流式实现时实现必要状态和算子；数值或完整性未通过的模式保持实验状态。
- 自回归采样存在漏读风险；固定中文语料转写和人工听感独立于吞吐验收。

## Migration Plan

已有 model=None 继续 Nano。通过验收的模式作为 moss 模型选项，各模式音色/缓存隔离；移除新增 feature 即回到现有功能。

试用构建给 Nano 增加显式 nano 目录 ID，保留旧 None 配置语义，以便同后端切回 Nano。完整大模型 CPU 未验收，worker 暂仅公开 GPU；没有 CPU adapter 时 Auto 只选择公开 GPU，不运行 CPU 对照或降级。Local 未达主力速度要求，在目录中注明实验、较慢；人工音质与 30 分钟验收独立记录。
