# 连续朗读、逐句对齐与设备加速验收

日期：2026-10-06。设备：Apple M4 Pro / macOS / ARM64。实现对应 OpenSpec `continuous-tts-alignment-acceleration`，尚未提交或发布。

## 架构与资源

```mermaid
flowchart LR
    Reader[阅读器 / 轻量协议 v3] --> Worker[worker 依赖组装]
    Worker --> Synthesis[MOSS 或 Kokoro 推理线程]
    Worker --> Alignment[独立 Qwen 对齐线程]
    Synthesis --> Core[core 原文映射 / 裁剪 / 有界缓冲]
    Core --> PCM[共享不可变 PCM]
    PCM --> Player[连续播放 / 原始音频帧时钟]
    PCM --> Alignment
    Alignment --> Timeline[句子时间线]
    Player --> Checkpoint[播放完成检查点]
    Timeline --> Checkpoint
```

core 不链接 ONNX、SentencePiece 或 Qwen；backends 各自持有模型与推理线程；CLI 的 preparation/synthesis、alignment、concurrency 管理资源与校准。阅读器只依赖 protocol，不处理张量。模块使用 foo.rs 和同名子目录。

- Qwen ONNX 固定 revision：`261c9ed100c1b18a4a1fbc488e05625dc9a4ae5c`。
- CPU：model_q4.onnx 1,048,397,349 bytes；tokenizer.json 11,429,733 bytes。
- GPU：model.onnx 1,917,718 bytes；外部 model.onnx_data 3,670,969,192 bytes；共用 tokenizer。
- 清单及 SHA-256：`crates/novel-tts-backends/src/alignment/resources.rs`。全部文件实测校验成功。
- MOSS：固定两个模型仓库 revision、16 个文件，共 763,191,513 bytes，清单在 moss/assets/resources.json。
- Kokoro V1.1：343,605,188-byte 模型及 54,001,874-byte voices，实下载计算 SHA-256 后固定在 models.rs。
- 下载总量来自清单，统一断点续传、文件锁、校验和损坏隔离；校验阶段也发送状态。

Qwen 的音频前处理、分词、prompt 和时间戳解码来自 [Qwen 官方实现](https://github.com/QwenLM/Qwen3-ASR/blob/main/qwen_asr/inference/qwen3_forced_aligner.py)，上游代码记录 commit `7c6daf77a2421100f5fb066495372c00129d39ff`；图来自 [固定 ONNX 导出](https://huggingface.co/valoomba/Qwen3-ForcedAligner-0.6B-ONNX/tree/261c9ed100c1b18a4a1fbc488e05625dc9a4ae5c)。上游以 Apache-2.0 提供；模型独立下载，用户运行无需 Python。

## 数值与生命周期

- Native Q4 与 Python ORT 1.22 同输入中文参考的全部 79 个 logits argmax 一致；浮点 CPU/CoreML 也一致。中文参考句末 3.280 秒。
- 原生 128-bin Whisper mel 的 512 个探针与 transformers 参考误差小于 0.0003。特征 mask 必须是 Int32，音频槽数按 100-frame/13-token 公式生成。
- 英文、混合文本原生推理完成，示例句末分别 3.440 / 5.920 秒；不能把这些结果当成人工边界精度验收。
- Qwen 端到端可选真实模型测试：seed=42 的 MOSS PCM → native mono/resample/mel → 分词 → Q4 → 与官方 token 和句末时间相等。
- MOSS 官方中文/混合 fixture 的 token、样本数、512 个 PCM 探针及取消后新任务通过。CoreML 同输入 PCM 相比 CPU 最大误差 2.4e-6。
- core 覆盖连续队列、暂停、跳转、取消、音色替换、失败块、不完整流、格式改变、共享预算、句子时钟和检查点。句子结果的来源范围、顺序和音频帧边界均校验。
- 真实 worker v3 三句输入已从片段切换逐句，发送两个后续句子的开始/结束，并正常完成；第一句对齐迟到后不倒退高亮。

音频持久保留预算为 30 秒 / 16 MiB，预取、播放与对齐共享 PCM 和预算租约；即使对齐调用超时，仍在推理线程中的 PCM 不提前归还预算。模型张量、重采样/mel 和边界处理的临时工作内存单独计量。播放时钟是混音器已消费的原始音频帧；尚未校正硬件输出缓冲延迟。

## M4 Pro 发布构建测量

模型已加载、同文同音色、3 次预热 + 5 次测量平均；不是模型下载/校验耗时。首音频计时包含边界裁剪。运行命令见下方，原始日志在忽略目录 target/alignment-research。

| 组件 | CPU | CoreML | 自动选择 |
| --- | ---: | ---: | --- |
| MOSS 完整合成 | 887.1 ms | 1165.8 ms | CPU |
| MOSS 首 PCM | 139.5 ms | 146.8 ms | CPU |
| Qwen 中文 3.6 秒输入完整对齐 | 344.8 ms | 423.8 ms | CPU |

CoreML 使用 MLProgram / All compute units / 静态子图。动态 shape MLProgram 的 E5RT 编译失败，静态子图路径成功。Qwen 浮点 profile：4028 个 CPU node、52 个 CoreML node；provider 可用不等于整个模型在 GPU/神经引擎执行。两项均不满足至少快 15% 的阈值，默认 auto 保留 CPU。缓存含硬件、模型 revision、ORT 信息；升级或更换硬件重新校准。并发校准代码完成，但该设备独立校准已拒绝加速，无需启用该对组合。

CUDA 使用 I/O binding 将 present KV 与 codec state 输出留在 CUDA，下一步直接复用；少量 hidden、采样及 PCM 输出仍在 CPU。ORT 原生包跟随固定 rc.10 的 CUDA12 下载清单，发行脚本必须找到原生 CUDA provider 库，否则拒绝打包；Linux 使用可执行文件同目录 rpath。CUDA/cuDNN 需与 [ORT 官方依赖表](https://onnxruntime.ai/docs/execution-providers/CUDA-ExecutionProvider.html#requirements) 匹配。仅通过 macOS 编译 feature 检查；无 NVIDIA 实机，不能宣称已经验证传输、算子分配、效率或发布包运行。

CPU Qwen 3+5 debug 测量的峰值 RSS 3,238,428,672 bytes，峰值 footprint 3,221,343,064 bytes；这是单独对齐进程的总内存，不能解释为整个 worker 的峰值。发布构建和并发内存另需测量。

## 同文试听

continuity example 使用 Weiguo 和 seed=42：原逐句合成 3 块 / 17.2 秒；合并上下文后 1 块 / 10.4 秒。输出 sentences.wav 和 continuous.wav，可用于主观听感对照。该离线对照的首次推理含不同冷热状态，不能直接当作公平延迟基准。自动测试确认只裁剪块首/块尾静音，块内停顿完整保留；硬件实际输出的块间静音尚未独立采集。

## 复现

```sh
cargo test --locked --all-features --workspace --lib --tests --examples
cargo clippy --locked --all-targets --all-features --workspace -- -D warnings
cargo fmt --all --check
RUSTDOCFLAGS='-D warnings' cargo doc --locked --no-deps --document-private-items --all-features --workspace --examples
TRNOVEL_MOSS_MODEL_DIR=<root>/moss TRNOVEL_QWEN_MODEL_DIR=<root>/alignment/qwen cargo test -p novel-tts-backends
cargo run --release -p novel-tts-backends --features coreml --example device_calibration -- <root>/moss
cargo run --release -p novel-tts-backends --features coreml --example alignment -- <root>/alignment/qwen chinese.wav '你好，欢迎使用听书功能。' cpu
cargo run --release -p novel-tts-backends --features coreml --example alignment -- <root>/alignment/qwen chinese.wav '你好，欢迎使用听书功能。' coreml
cargo run -p novel-tts-backends --example continuity -- <root>/moss output-directory
```

24 种 CLI 后端/对齐/设备 feature 组合，以及无 TTS 阅读器、无后端 CLI，共 26 种 cargo check 通过。全 workspace 测试、所有 target / feature 的 Clippy（warnings deny）、rustdoc（warnings deny）、rustfmt、git diff --check 和文档站构建通过。VHS 已查看真实 MOSS + Qwen 的片段转逐句高亮、暂停后高亮保留、设备状态与一位小数显示；无 TTS 构建布局正常。录制须清空 NO_COLOR，避免环境变量关闭主题高亮。

## 保留未验收

- 人工标注边界误差 median ≤200 ms / P95 ≤500 ms：尚无独立人工标注，保持未验收。
- NVIDIA 实机、CUDA 原生包运行及真实 GPU KV/profile：缺少硬件，保持未验收。
- 用户主观试听、长时间播放、音频设备失效故障注入、硬件输出延迟与完整 worker 峰值内存：保持未验收。
- 加速发布工作流只新增配置，未上传、发布或执行远端 CI；不把本机 feature 编译等同于跨平台发行验收。
