# Design

## Context

现有后端以专用推理线程和有界 PCM 通道实现 Backend。MOSS/Kokoro 使用 ORT；Qwen 使用 Candle，设备支持不能直接继承 ORT provider 列表。

## Goals / Non-Goals

提供可选择、可取消、带资源校验的 Qwen 0.6B CustomVoice，复用播放和原文坐标。首版不加入 Base 克隆、VoiceDesign、多模型配置或 Qwen CUDA。

## Decisions

- 固定 TrevorS/qwen3-tts-rs revision 711ceee07cad92673f86de8997bdf54c30caa49f；固定官方模型 revision 85e237c12c027371202489a0ec509ded67b5e4b5，全部资源记录 SHA-256 和尺寸。
- qwen.rs 实现适配器，qwen/resources.rs 管资源，qwen/text.rs 管分段。模型在线程内构建、销毁，通道容量一；每次 next_chunk 前检查取消。上游一次调用生成十帧，取消粒度是音频块，不承诺逐帧中断。
- 只有 StreamingSession::is_done() 且已有有效 PCM 才发布 End；达到帧数上限不写完成检查点。
- 按真实段落与标题分组，软换行在合成副本去除；独立使用 180 UTF-8 字节预算（最多约 60 个汉字）。原文坐标保留。中文输入选择 Chinese，其余首版选择 English。
- Registry 提供按后端的编译/可用设备信息。Qwen 提供 CPU 和显式 metal feature；CoreML/CUDA 对 Qwen 明确拒绝。对齐器仍独立选择 ORT 设备。
- auto 沿用 3 次预热、5 次测量与收益阈值，缓存绑定模型与实现版本；资源缺失时才下载。

## Risks / Trade-offs

上游标记为实验实现，流式 codec 小块独立解码，边界音质需试听；不宣称正常 EOS 可以证明完整朗读。CPU 性能、Metal 算子支持和模型内存必须实测。Metal 依赖按 macOS target 启用，其他平台不报告 Metal；同时启用 Candle core/nn/transformers 的 Metal 算子，不能只验证模型加载。

## Validation

单元测试覆盖音色、设备、文本映射、帧数失败；真实模型检查 EOS/PCM/取消并输出试听。运行质量检查、后端组合和 UI 检查，无法完成的音质或硬件验收明确保留。
