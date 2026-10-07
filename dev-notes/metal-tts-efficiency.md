# Metal TTS 效率实测 — 2026-10-07

本机六种候选的预热后 RTF 都大于 1，尚不能保证实时连续听书。OmniVoice 初始样本是噪音，原效率记录作废；表格已替换为修复后的有效语音复测。VoxCPM2 首 PCM 快，持续吞吐不足；MOSS Local 最慢。这是当前实现、默认生成参数和本机的结果，不代表 Metal 的理论上限。

## 环境与测量

- 分支 `tts-gpu-model-integration`，远程基线 `a7aa9a3`，加本轮 macOS 构建、设备发现和 OmniVoice 排序修复。
- M4 Pro：12 CPU 核（8P+4E）、16 GPU 核、24 GB 统一内存；macOS 26.6.2（25G83），接电。Rust 1.98.1，Candle 0.9.2，release/LTO。
- 测量时没有 Rust 编译或其他测试推理并行，普通桌面程序保持打开。
- 短输入：“山风吹过松林，星光照亮归途。两个人沿着小路慢慢往前走。”所有模型 seed=42；Qwen 用 `uncle_fu`，其他用 `narrator`，无克隆参考。
- 使用实际生产适配器。同一加载实例先预热一次，再测三次，表格为中位数。修复后的 OmniVoice 使用三个独立进程，每个进程先完成一次 PCM 预热，再测一次，两次共享模型实例。关闭对齐；直接消费 PCM，不播放、不引入播放通道背压。
- RTF = 生成耗时 / 实际可播放音频时长。边界裁剪采用 core 相同的 10ms 窗、peak/RMS 阈值 -45/-55dB，保留最多 100ms 开头静音和 150/300ms 尾部静音。短输入为段落结尾；长正文逐段裁剪。原始 PCM 指标同时保留。
- 首 PCM 包含本请求文本处理/分段，排除模型加载；不等于播放器预缓冲后的起声时间。OmniVoice 首 PCM 要等待整段生成。
- 内存采用 `/usr/bin/time -l` 的 peak memory footprint，单位 GiB（2^30 bytes），包含初始化瞬时峰值；RSS 单独保存。统一内存不能当成专用 CUDA 显存，也不能与整机 GPU 内存直接相加。

## 短输入预热后结果

| 模型 | 首 PCM s | 生成 s | 可播放 s | RTF | 加载 s | 峰值内存 GiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Qwen 0.6B | 0.887 | 10.658 | 8.45 | 1.261 | 0.886 | 5.65 |
| Qwen 1.7B | 1.058 | 10.345 | 8.16 | 1.268 | 2.412 | 8.88 |
| VoxCPM2 2B | 0.314 | 11.580 | 6.88 | 1.683 | 2.290 | 3.34 |
| OmniVoice 0.6B（已修复） | 5.666 | 5.666 | 5.40 | 1.049 | 0.157 | 3.73 |
| MOSS Realtime 1.7B | 0.601 | 9.998 | 6.96 | 1.436 | 24.315 | 12.38 |
| MOSS Local 1.7B | 1.152 | 23.451 | 8.24 | 2.846 | 25.700 | 14.90 |

Qwen 0.6B 原 PCM 为 10.00s，裁剪后 8.45s，RTF 从原始 1.066 变为 1.261。其他有效短输入未触发裁剪。Qwen Metal 使用 BF16；MOSS 模型和 codec 使用 F16。Vox 固定 Q8_0 BaseLM/F16 Acoustic。未调整采样、步数或音质参数。

## 同机 CPU 对照

| 模型 / 设备 | 首 PCM s | 可播放 RTF | 峰值内存 GiB |
| --- | ---: | ---: | ---: |
| Qwen 0.6B / CPU | 2.549 | 3.662 | 6.98 |
| Qwen 0.6B / Metal | 0.887 | 1.261 | 5.65 |
| VoxCPM2 / CPU | 0.541 | 2.654 | 4.91 |
| VoxCPM2 / Metal | 0.314 | 1.683 | 3.34 |

按实际可播放时长归一，Qwen 0.6B 约 2.90 倍、VoxCPM2 约 1.58 倍收益。浮点差异影响音频/EOS，CPU 与 Metal 输出时长不同；这是当前适配器默认参数的对照，不是硬件峰值性能。Vox CPU 日志确认 CPU backend、零 GPU offload；Metal 为 MTL0、29/29 BaseLM 层 GPU offload，声学组件也在 MTL0。

## 较长固定语料

输入 `tools/tts/acceptance-corpus.txt`（285 字符、214 CJK），包含对话、数字/日期、英文和小说正文。每组在新加载实例上执行一次完整语料，没有额外预热；每段要求显式 End。

| 模型 | 片段数 | 生成 s | 原 PCM s | 可播放 s | 可播放 RTF | 峰值内存 GiB |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| VoxCPM2 | 11 | 105.882 | 61.60 | 60.45 | 1.752 | 3.34 |
| Qwen 1.7B | 8 | 94.653 | 74.08 | 74.08 | 1.278 | 9.40 |

Vox 原 PCM RTF 为 1.719，逐段裁剪后为 1.752；Qwen 两种指标均为 1.278。长正文没有改变短段结论。导出 WAV 均为有限、非静音 PCM，各段正常 End；这些指标不证明逐字覆盖或音质。本轮未做 30 分钟连续播放和人工逐字验收，OpenSpec 组合验收项仍未完成。Windows 记录使用不同音色/输入及部分不同分块参数，不计算严格的 CUDA/Metal 跨机器加速比。

## Mac 修复与检查

1. `crates/voxcpm-sys/build.rs`：补链接 `ggml-blas`。Apple CMake 默认启用 BLAS，缺少静态库导致最终链接 `_ggml_backend_blas_reg` 未定义。
2. `crates/voxcpm-sys/native/bridge.cpp`：原生注册名是 `MTL`、实际 backend 是 `MTL0`；设备发现和防 GPU 回退检查此前查找 `Metal`，导致真实模型无法加载。CUDA 名称匹配不变。
3. 早期曾修复 ZipVoice vendored eSpeak 的 `std::sample` 歧义；随后按用户决定移除整个 ZipVoice 后端及该工具，当前构建不再需要这项修复。

OmniVoice 修复后，最终工作区 531 测试通过、1 个既有条件测试跳过；all-targets/all-features Clippy（warnings denied）、fmt、examples rustdoc 均通过。实际 release 阅读器/worker 构建、help、协议 Hello/GetConfig/Shutdown 与模型目录查询通过。目录列出 Qwen、Vox、Omni 和两种 MOSS 的 Metal 设备。测试配置隔离，没有改变用户偏好。修复和本报告未提交。

## OmniVoice 噪音修复与复验

人工反馈揭示初始样本全是噪音。之前的有限、非零 PCM 与正常 End 检查不足以认定音质有效，原记录 5.711s / 5.40s / RTF 1.058 作废。

根因是 Candle 0.9.2 Metal `arg_sort_last_dim` 的单线程组 bitonic sort：排序列数向上取 2 次幂作为线程数，1040 个 token 需要 2048 线程，超过 Metal 的 1024 限制，损坏的位置索引使 1039/1040 个 mask 未被替换。普通解码没有范围检查，Metal embedding 将越界 token 截断成有效索引，最终仍产生有限非零噪音。强制 stage0 F32 同样失败，stage1 本来就用 F32。

修复只对 Metal 且列数 >1024 的分数排序使用 CPU，返回索引到原设备；模型前向、更新与声码器仍在 Metal。位置选择和可选 class top-k 共用保护，普通 stage1 解码补范围检查。CUDA 行为不变。

- 回归测试：1040 位置的部分／全部替换，1024/1025/4097 列 class top-k，均与 CPU 一致。位置测试在修复前失败、修复后通过。
- 真实模型：修复后的 1040 token 全部在 [0,1023]，不再残留 mask。同一组 token 在 CPU／Metal 解码，raw waveform 相对 L2 误差 1.58e-6、cosine 0.9999999999987，各解码层一致。
- 独立内容检查：whisper.cpp small，无原文提示，旧 `omni-metal-1.wav` 只识别为“音乐”；新 `omni-debug/fixed/live.wav` 识别为“山峰吹過松林 星光照亮歸途 / 兩個人沿著小路慢慢往前走”。仅“山风”出现同音字差异。这证明样本包含目标语音，仍不能代替所有音色／长篇人工音质验收。
- 实际适配器三轮预热复测：生成 5.666s 中位数、PCM 5.40s、RTF 1.049（范围 1.038–1.055），peak footprint 3.73GiB。三轮均正常 End；第二轮适配器 WAV 的独立识别也得到上述正文。CPU 排序修复没有在此输入上造成明显额外耗时，尚未达到实时持续吞吐。
- 诊断源码、前后回归日志、层级对照、WAV、识别结果与临时 ASR 模型保存在 `target/metal-efficiency/omni-debug/`；ASR 工具不进入产品依赖。TTS 模型仍保留在默认目录。

复测命令（先生成一次 PCM 预热，再测一次）：

```bash
NOVEL_TTS_PROBE_CANCEL=1 target/release/examples/native_probe \
  omnivoice "$HOME/.novel-tts/omnivoice/models/0.6b/c5fdb5ccb189668d56333f77ba2629f4cd7535f4" \
  target/metal-efficiency/omni-fixed.wav metal @target/metal-efficiency/short.txt narrator
```

`omni-fixed-metal-{1,2,3}.*` 为最终生产适配器证据；`summary.json` 将旧 `omni-metal` 标记为 `valid_for_performance=false`，新增 `omni-fixed-metal`。重新启动 `target/release/trn` 会使用已重建的同目录 worker；原模型无需重下。

## 默认缓存与手测

仓库固定 revision、size、SHA-256 的权重已完整校验，保存在 `~/.novel-tts/` 默认目录。既有 Qwen 0.6B 也重新校验；MOSS 共用一套 codec。下载日志已移出模型目录。

已就位：Qwen 0.6B / 1.7B CustomVoice、VoxCPM2 2B、OmniVoice 0.6B、MOSS Realtime / Local 1.7B 与 codec。其他 Qwen 档位、MOSS VoiceGenerator、ZipVoice 权重和可选对齐模型不在本轮下载范围。

```bash
./target/release/trn
# 在听书设置选择模型和 Metal；同目录 novel-tts 包含全部测试 GPU feature。
```

worker feature：`metal,voxcpm-metal,omnivoice-metal,moss-candle-metal` 加默认 `moss,alignment`。准备环境通过 Homebrew 安装 CMake 和 aria2，未修改全局环境变量或 shell profile。

## 证据与复测

`target/metal-efficiency/` 保存 `summary.json`、`audio-check.json`、逐模型 `.jsonl` / `.log` / `-resources.json`、WAV 和分段索引。`download.log` 以 ALL VERIFIED 结束。`qwen06-smoke` 与编译并行，仅用于初次可用性检查，不计入结果。早期中断的 CPU 测量已被最终四轮成功测量替换；初始 Vox 设备发现失败也被修复后的成功测量替换。

临时重复测量入口源码保存在该目录的 `metal_bench.rs`，编译产物仍可复用；仓库 `native_probe` example 支持单轮复测。

```bash
METAL_BENCH_RUNS=4 target/release/examples/metal_bench \
  voxcpm "$HOME/.novel-tts/voxcpm/models/2b-q8_0/169f64d8b98bbaab1761e4ca3a83e6af653456cc" \
  /tmp/voxcpm-metal.wav metal @target/metal-efficiency/short.txt narrator
```

## 后端收敛（同日后续决定）

Kokoro 与 ZipVoice 已移除，CPU 默认仅 MOSS Nano。worker 启动时将两者的旧配置及缺少 backend 的旧配置迁移为 Nano 默认音色、CPU，保留其他偏好与未知字段。用户模型缓存不删除；ORT 仍用于 Nano 与对齐。锁文件移除 35 个不再可达的依赖包，没有升级其他依赖。
