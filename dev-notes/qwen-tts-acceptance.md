# Qwen3-TTS Candle 集成验收（2026-10-06）

## 范围与依赖

集成前的 MOSS、连续播放和默认关闭对齐改动已提交为 `f2de440`。本变更增加可选 `qwen` 后端，不改变 MOSS 默认值。采用 [TrevorS/qwen3-tts-rs](https://github.com/TrevorS/qwen3-tts-rs) 的原生 Rust/Candle 实现，固定 revision `711ceee07cad92673f86de8997bdf54c30caa49f`，Candle 0.9.2。无需 Python 推理环境。

模型为官方 `Qwen/Qwen3-TTS-12Hz-0.6B-CustomVoice`，revision `85e237c12c027371202489a0ec509ded67b5e4b5`，8 个文件合计 2,498,383,173 bytes。manifest 固定大小和 SHA-256，资源保存在 `~/.novel-tts/qwen/`，与 Qwen 强制对齐资源分开。9 个预置音色，默认福叔；首版语言策略为中文和英文，未接入参考音频克隆、VoiceDesign 或 Candle CUDA。

CPU 与 macOS Metal 使用后端自己的设备目录。Metal 同时启用 Candle core、nn、transformers 的算子支持：单独启用 core 可以加载模型，但实际 RMSNorm 推理会失败。本次已复现并修复该构建问题。依赖仅针对 macOS，其他 target 不报告 Metal。现有 ORT/Kokoro 版本保持不变。

协议升级为 v4；阅读器与 worker 需同步升级。阅读器不依赖 Candle、具体后端、core 播放库或 ORT。MOSS/Kokoro/Qwen 共用模型无关的有界 PCM、播放和检查点机制。

## 实测环境与性能

Apple M4 Pro、24 GiB 内存，release 构建。固定文本、福叔和随机种子 42；CPU F32 与 Metal BF16 的输出帧数可能不同，不宣称 PCM 一致。

| 设备 | 模型加载 | 首 PCM | 完整生成 | 音频时长 | 最大 RSS |
| --- | ---: | ---: | ---: | ---: | ---: |
| CPU | 1.855 s | 2.956 s | 11.819 s | 3.68 s | 约 5.05 GB |
| Metal | 0.546 s | 0.896 s | 3.787 s | 3.52 s | 约 5.08 GB |

输入为“你好，欢迎收听。”，结果和试听在 `target/qwen-acceptance/{cpu,metal}-seed42.{json,wav}`。这组短文本仅为诊断；自动设备选择另行执行完整链路 3 次预热、5 次测量：

| 设备 | 平均完整生成 | 平均首 PCM |
| --- | ---: | ---: |
| CPU | 19.9253 s | 2.5965 s |
| Metal | 13.2949 s | 0.8946 s |

完整生成降低约 33%，首 PCM 降低约 65.5%，通过至少 15% 收益且首 PCM 不回退超过 10% 的门槛，auto 选择 Metal。首次校准到模型就绪约 289 s；缓存命中后的实际 worker 准备约 8.07 s（包含校验，且与其他验证并发）。缓存按硬件、模型和运行库隔离，并为不同模型使用不同文件名。

## 自动验证

- 完整 workspace 279 项测试、Clippy warnings deny、rustfmt、rustdoc warnings deny。
- 8 组构建：Qwen CPU、Qwen Metal、Qwen + alignment、Qwen + alignment + Metal、MOSS + Qwen、Kokoro + Qwen、全部后端与设备、无 TTS 阅读器。
- Qwen 单后端协议测试：设备与后端校验、9 音色目录，无需下载模型。
- 真实模型测试：首 PCM 为 24 kHz、取消接收后释放当前生成，下一请求正常 EOS；真实测试耗时约 13 s。
- EOS 单元测试：达到帧数上限、空音频不会发布完成事件；上游 `next_chunk()==None` 不能单独作为正常完成证明。
- 原文 UTF-8 映射、软换行合并、硬边界、装饰过滤、长文本字符边界切分。
- 模型文件全部 SHA-256 校验；关闭 alignment 时未创建对齐任务或加载对齐模型。

## 限制与未验收项

上游为实验实现，小块 codec 解码的听感仍需人工试听。正常 EOS 不能证明每个字都已朗读；中文、英文、混合文本、长文本与块边界的人工内容和音质验收继续保留。NVIDIA、Linux、Windows 实机未验收；本次不提供 Qwen CUDA。

所选上游目前通过固定 Git revision 引入，尚无对应 crates.io 包。独立发布 novel-tts-backends 前需要解决依赖分发；本次不宣称 crates.io 发布可用。现有 MOSS/Kokoro feature 和默认行为保留。

长段落 Metal 探针：“山风吹得血袍飘荡……”正常 EOS，首 PCM 0.893 s、生成 18.882 s、音频 17.68 s；模型加载 0.606 s。试听与指标在 `target/qwen-acceptance/story-metal.{wav,json}`。这仅验证生成完成，内容与听感仍需人工核对。

VHS 已检查后端 Qwen、Serena 音色切换、Auto → Metal、模型就绪，以及实际 Playing 状态和绿色片段高亮。暂停与退出后检查点保留在完成位置。录制在 `target/qwen-acceptance/vhs/qwen.gif`，可复用脚本为 `docs/tapes/qwen-tts.tape`。

## 本地库与 CUDA 接入（2026-10-07）

推理源码迁入 `crates/qwen3-tts`，来源固定 revision 不变。推理用 tokenizers 禁用 `esaxx_fast`，Windows Qwen + ORT CUDA 构建不再要求 `/MD` 环境变量。增加 `qwen-cuda`，原 ONNX `cuda` feature 改为 `ort-cuda`；GPU 设备目录与加载共享本地设备创建入口。移除上游 Hub、CLI、手写 fused PTX 和 Flash Attention，残差归一化使用 Candle 自带设备算子。

本机 CUDA 12.9 Update 1 已安装到 `D:\dev-tools\cuda\v12.9`，官方组件 SHA-256 全部校验通过。nvcc 12.9.86 与 MSVC 14.44 编译的 `sm_120` 内核在 RTX 5070 实际运行成功，官方 deviceQuery / vectorAdd 通过。`cargo build --locked -j 2 -p novel-tts --no-default-features --features qwen-cuda` 通过，worker 协议探针确认 Qwen `compiled` 与 `available` 均包含 CUDA。日志在 `target/qwen-local-cuda-build.log` 与 `target/cuda-install-check/worker-probe.log`。

初次 CUDA-only 构建产生 LNK4098；默认 MOSS + Qwen CUDA 组合进一步暴露 Candle MOE `/MT` 与 ORT `/MD` 的 LNK2038。将 MSVC x64 编译器目录与 CUDA bin 写入系统 PATH，配置系统 `NVCC_PREPEND_FLAGS=-Xcompiler=/MD` 并重建 candle-kernels 后，无需 PowerShell 激活脚本的 `cargo build --locked -j 2 -p novel-tts -F qwen-cuda` 通过，未产生链接警告。dumpbin 确认 MOE 对象的 RuntimeLibrary 为 MD_DynamicRelease；日志在 `target/qwen-local-cuda-default-build.log`。没有使用 `/NODEFAULTLIB` 屏蔽。

2026-10-07 后续已完成 Windows RTX 5070 的 0.6B/1.7B CustomVoice、Base/VoiceDesign 实际生成、正常 EOS、取消后重试以及 1.7B CustomVoice 30 分钟实际播放。RTF 分别约 0.758、0.716、0.796；30 分钟 RTF 约 0.758，显存未见持续增长。当前协议为 v5，新增模型策略与完整证据见 `tts-model-tiers-acceptance.md`。人工听感、Linux CUDA 与本次新增模型的 macOS Metal 实机仍待验收。OpenSpec 中将这些验收项分别记录，历史 Metal 测量保留为历史数据。

本地 portable feature 集的 workspace lib/tests/examples 回归通过（488 次测试通过，1 次忽略，其中包括既有协议测试对子进程的测试调用）；Clippy `-D warnings`、rustfmt 和 rustdoc `-D warnings` 通过。检查 reader 基础依赖树没有 Candle/ORT/rodio/core，Qwen-only 没有 ORT，esaxx-rs 未启用 cpp。通用 CI/lefthook 改用显式 feature 集；独立 qwen-cuda.yml 使用 CUDA 12.8.1 devel 环境和 CC 120 编译检查，该工作流尚未触发或验证。
