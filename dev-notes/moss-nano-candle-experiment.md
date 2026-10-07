# MOSS Nano Candle 实验（2026-10-07）

基线提交 `1ab87ee`，实验分支 `codex/moss-nano-candle`。结论：Nano 的全链路合成可以运行在 Candle CPU/Metal 上，18 个内置音色和旧自定义音色可继续使用。M4 Pro 上 Metal 的生成吞吐约为 Candle CPU 的 2.3 倍，首块更快；当前优化后的 ONNX CPU 的总生成吞吐仍然更高，因此保留 ONNX CPU 默认。

## 实现与缓存

- 计算：GPT2 global 12 层 + local 1 层；16 RVQ 码本；F32 推理。
- 直接读取官方 BF16 PyTorch checkpoint，不调用 Python，不做部署时权重转换。
- 解码：Candle causal codec，48 kHz 双声道交错 PCM，每次 3 帧，保留流式状态。
- 模型项 `moss/nano-candle`，feature `moss-nano-candle`；设备 feature `moss-nano-candle-metal` / `moss-nano-candle-cuda`。CUDA 仅编译门控，本机没有 CUDA 实测结果。
- 默认目录 `~/.novel-tts/moss/models/nano-candle/44502f80dbf9743528fa921cc544d662c685ebec/`，资源 manifest 固定 revision、文件大小和 SHA256。
- TTS 来自 [OpenMOSS-Team/MOSS-TTS-Nano-100M](https://huggingface.co/OpenMOSS-Team/MOSS-TTS-Nano-100M/tree/44502f80dbf9743528fa921cc544d662c685ebec)，codec 来自 [MOSS-Audio-Tokenizer-Nano](https://huggingface.co/OpenMOSS-Team/MOSS-Audio-Tokenizer-Nano/tree/6aa02b01e445cc585582cf0ba480bc3ea6c8dd68)。原始资源总计约 324 MB。
- 架构参考 [OpenMOSS 官方实现](https://github.com/OpenMOSS/MOSS-TTS-Nano)，GPT2 attention 约定交叉参考 [社区 Candle 实现](https://github.com/ramishi/moss-tts-nano-rust-candle/tree/f4d3fcf4de9b4118ee664087f88495f4174836e1)。没有使用社区 gated 权重。
- Nano 内置/自定义音色复用 `moss/voices` 的旧 16 码本格式；CLI voices import 当前仍复用 ONNX 编码入口。TTS 合成和流式解码都由 Candle 执行。

## M4 Pro 速度

无并行 Cargo 构建或其他本任务推理。release 构建，Weiguo，seed 42，每次单独启动 adapter。RTF = 生成耗时 / 音频时长，越低越快；加载时间单列，未计入 RTF。

短句「你好，欢迎使用听书功能。」各 3 次取中位数：

| 路径 | 加载 ms | 首块 ms | 生成 ms | 音频秒 | RTF | 峰值进程 RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| ONNX CPU | 1073 | 148 | 665 | 3.60 | 0.185 | 1189 |
| Candle CPU | 124 | 318 | 2385 | 3.92 | 0.608 | 755 |
| Candle Metal | 143 | 85 | 1040 | 3.92 | 0.265 | 428 |

长段「今天我们测试新的听书模型。它应该保持自然的语气，读清楚每一个字，并且在切换音色后继续正常工作。」各 1 次：

| 路径 | 加载 ms | 首块 ms | 生成 ms | 音频秒 | RTF |
| --- | ---: | ---: | ---: | ---: | ---: |
| ONNX CPU | 1098 | 149 | 2076 | 12.32 | 0.169 |
| Candle CPU | 127 | 321 | 8129 | 12.96 | 0.627 |
| Candle Metal | 154 | 99 | 3583 | 12.96 | 0.276 |

Candle 两种设备采用相同采样逻辑，输出长度一致。ONNX 融合图与原生路径的采样输出仍有差异；上述 ONNX/Candle 比较只说明单位音频长度的吞吐，不声称逐帧/逐字音质等价。进程 RSS 由 macOS `/usr/bin/time -l` 读取，不能代表 Metal 独占显存或系统总统一内存。ONNX adapter 还加载语音导入 encoder 等会话，所以其加载/RSS 是现有完整 adapter 的成本。

原始数据和 WAV：`target/tts-integration/nano-idle-bench/`。固定种子短句 Candle CPU/Metal 的波形比较为 376320 samples，最大误差 `9.5367e-7`，RMSE `6.8247e-8`，相关系数 `0.999999999998`。这证明两条计算路径数值一致，主观音质仍应试听。

## 验证

- 工作区 all-features：535 passed，1 ignored，0 failed。
- 工作区 Clippy（warnings denied）、rustfmt、rustdoc（warnings denied）通过。
- CPU-only worker feature 构建通过；带所有现有 Metal 后端与实验 Nano 的 release worker 已重新构建。
- GPT2 小型 PyTorch 数值 fixture：prefill、分块 KV cache、GELU-new、RoPE 与取消校验通过。
- 真模型 codec：65 帧固定 Weiguo 音频码、3 帧流式分块，CPU 与 Metal 的 1024 个采样点都与 pinned ONNX codec 在 `3e-4` 阈值内；包括后续 stage 的缓存回绕。24 kHz 旧 codec 数值回归通过。
- CPU/Metal 取消后下一次生成成功；owner 关闭且音频队列仍存活时都约 9 ms 释放。满队列取消有独立回归测试。
- 真实 release worker：ONNX → Candle CPU → Candle Metal → ONNX，4 次 prepare/play/切换均 `session_ended=completed`，alignment 关闭，音量 0，使用隔离配置和检查点。记录：`target/tts-integration/nano-worker-acceptance/`。
- `voices list --backend moss --model nano-candle` 返回 18 个内置音色。

## 手测

仓库根目录运行阅读器，并指定刚构建的 worker；在模型设置里选择「MOSS Nano Candle（实验）」、Metal 和所需音色：

```sh
cargo run -- --tts-program "$PWD/target/release/novel-tts"
```

独立 CLI 也可运行，模型不需要重新下载（`book.txt` 替换为 UTF-8 小说路径）：

```sh
target/release/novel-tts --backend moss --model nano-candle --voice Weiguo --tts-device metal book.txt
```

复现实验 probe：

```sh
cargo build --release -p novel-tts-backends --example moss_nano_probe --features moss-nano-candle-metal
target/release/examples/moss_nano_probe candle "$HOME/.novel-tts" /tmp/nano-metal.wav metal '你好，欢迎使用听书功能。' Weiguo
# 对照：第一个参数改为 onnx，设备为 cpu；Candle CPU 的设备改为 cpu。
```

运行真 codec 数值测试（需已缓存官方权重）：

```sh
TRNOVEL_MOSS_NANO_CANDLE_DIR="$HOME/.novel-tts/moss/models/nano-candle/44502f80dbf9743528fa921cc544d662c685ebec" cargo test -p moss-tts --features metal --lib
```
