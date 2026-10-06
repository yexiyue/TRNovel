# Design

## Context

当前 PCM / End 流和 sink.empty 只提供块边界。MOSS 原文范围准确，但合成、播放和检查点共用切分单位；异步对齐必须使用实际播放帧位置。

## Goals / Non-Goals

Goals: 流式首音频、连续块队列、句子对齐、CPU/CoreML/CUDA 独立选择、可信回退。
Non-Goals: Python 运行时、动态插件、按字数伪造时间戳。

## Decisions

- 合成目标 12 秒、预计上限 20 秒；50 token / 60 CJK 字符 / 375 帧仍为硬安全限制。初始 CJK 4 字/s、英文 2.5 词/s，更新当前音色估算并在换音色时重置。
- 保持自然段上下文、原文 UTF-8 映射和装饰行过滤；预算超限优先完整句末，其次分句。
- 10 ms 静音检测窗，RMS ≤ -55 dBFS、peak ≤ -45 dBFS；块首最多 100 ms，同段末 150 ms、段落末 300 ms；内部停顿不动。
- 共享不可变 PCM，合成块 / 句子时间线分离；播放标记驱动 checkpoint，不等待队列 drain 才接下一块。
- 独立 Aligner trait；Qwen CPU Q4、GPU 浮点 ONNX，先验证固定 ORT 的加载、推理和参考精度，再接入生产。
- 对齐不阻塞首音频；一个待处理任务，过期任务跳过；迟到结果不倒退。失败明确片段高亮，进度按实际已播放的可验证边界提交。
- 协议 v3；TTS / aligner 各有 auto/cpu/coreml/cuda。可选 Cargo coreml/cuda，worker 装配，reader 只消费协议。
- auto 校准：3 次预热、5 次测量；完整链路提速 ≥15%、首音频回退 ≤10%。缓存按硬件 / 模型 / ORT 版本失效；同时检查并发竞争。
- CoreML MLProgram 编译缓存；CUDA KV 和中间状态留在设备端。自动回退报告原因、不重放已输出音频；显式不可用设备报错。
- 固定模型 revision、bytes、SHA256，资源 ~/.novel-tts/alignment/qwen/。CUDA 无硬件实测则保留未验收。

## Risks / Trade-offs

社区 ONNX Q4 未证明逐句精度，GPU 混合分区可能比 CPU 慢，浮点图外部权重为约 3.67 GB。兼容性或精度门槛失败时停止该模型接入并报告证据；不改变 pinned ORT 来规避失败。
