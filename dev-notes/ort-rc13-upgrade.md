# ORT rc.13 升级验收

日期：2026-10-07；环境：Apple M4 Pro、macOS。本轮结果仅代表当前 Mac。

## 分支与范围

- 实验分支 `codex/moss-nano-candle` 已提交为 `8be7d95`，保存 Nano Candle 及 Qwen/OmniVoice ONNX 实验与报告。
- 回到 `tts-gpu-model-integration`，升级 workspace 依赖锁文件并适配 ORT API。实验实现没有合并到 TTS 分支。
- `ort` / `ort-sys` 固定到最新 `2.0.0-rc.13`；普通预编译原生库为 ONNX Runtime 1.28。保留 Candle 0.9.2 和现有默认模型/配置，不删除模型缓存。
- 用户授权不再维护 Intel Mac：移除基础版、听书版发布目标及 shell/npm 安装器对应映射，不引入旧 ORT 下载兼容路径。发布配置通过 cargo-dist 0.32.0 重新生成并检查。
- Windows/Linux CUDA 原生分发改用 CUDA 13；Windows 的 CUDA/cuDNN 安装和真实推理仍需在 Windows 验收。

上游版本与分发约束见 [ORT releases](https://github.com/pykeio/ort/releases)。

## 实现要点

使用新 `ort::ep`、`session::RunOptions`、`Session::inputs()/outputs()` API。SessionBuilder 错误携带不满足 Send/Sync 的 builder，转换成保留错误消息的 anyhow 错误后正常释放。设备注册使用 `error_on_failure()`。

显式启用 Rust API 21，避免 rc.13 在 API 22 及以上自动启用 MaxEfficiency 设备策略；应用仍自己校准、选择并注册设备。这个 API feature 选择没有降级原生预编译库，运行日志确认其为 1.28。musl 继续使用系统 ONNX Runtime（发行脚本要求 1.22 或更新），环境变量迁移为 `ORT_LIB_PATH`。

`lax-feature-matching` 允许 Mac 上编译含 CUDA 的 all-features 检查，但运行时仍验证 provider 可用性。CUDA 打包脚本拒绝缺少 CUDA provider 原生库的分发，编译通过不等于 CUDA 可运行。

ORT 1.28 的 CoreML MLProgram 路径在 MOSS 生成时出现内部 Shape 输入缺失和 Slice 参数维度错误；按新原生库版本隔离缓存后仍失败。改用 NeuralNetwork 格式完成生成与对齐，缓存目录按格式和原生库信息 SHA-256 隔离，已有缓存保留。没有以 EP 注册成功作为加速证据。

## 原生运行记录

输入“你好，这是语音合成测试。”，MOSS Nano、Weiguo、种子 42。以下是单次功能 smoke 数据，不是三次中位数性能基准，也不能用来证明音质相同。

| 路径 | 加载 | 首段 PCM | 生成 | 音频时长 | RTF |
| --- | ---: | ---: | ---: | ---: | ---: |
| ONNX CPU | 1.285 s | 101 ms | 690 ms | 3.600 s | 0.192 |
| CoreML NeuralNetwork | 4.675 s | 125 ms | 778 ms | 2.880 s | 0.270 |

两份 WAV 均完成正常 End，48 kHz、双声道浮点 PCM，样本全部有限且非零。CPU peak 0.651、RMS 0.0374；CoreML peak 0.172、RMS 0.0212。相同种子在不同 provider 下不保证同样的采样轨迹，音频时长也不同；没有把这些检查等同于人工听感通过。本次 Nano 数据未显示 CoreML 优势，CPU 默认保持不变。

使用 CPU 生成的同一 WAV 测 Qwen 对齐器：CPU 约 347 ms、CoreML 约 180 ms；两者都输出合法文本范围与时间戳（末尾分别 2.400 s、2.560 s）。这是单句 smoke 的计时和数值差异记录，不代表完整对齐精度评估。

试听文件位于仓库忽略的 `target/tts-integration/ort13-moss-cpu.wav` 和 `ort13-moss-neural.wav`；模型沿用 `~/.novel-tts/moss`，无需重新下载。

## 验证

最终检查状态和新缓存的重复运行数据见下方验收补记。Windows CUDA、GNU Linux 和 musl 原生运行尚未在本次 Mac 环境验收。

### 最终验收补记

- workspace 全 features 测试：530 passed、0 failed、1 ignored。
- Clippy 全 targets/features `-D warnings`、rustfmt、rustdoc `-D warnings`、`git diff --check` 全部通过。
- release worker（含 CoreML、Qwen/OmniVoice/MOSS Candle/VoxCPM Metal）构建通过；协议握手与退出通过。
- 发布 smoke 脚本原先仍使用协议 1，当前协议已经是 5。已改为读取源码 `PROTOCOL_VERSION`，避免后续漂移；没有修改协议本身。
- 独立新 CoreML 缓存：MOSS 加载 4.659 s、首 PCM 123 ms、生成 729 ms、音频 2.880 s；对齐约 155 ms，文本范围 0..36、时间 0..2.560 s。重复 WAV 与前一份 NeuralNetwork WAV 完全相同，PCM 全部有限且非零。
- Apple Silicon 基础版 shell/npm 本地安装通过；cargo-dist 0.32.0 `generate --check` 通过，release plan 共 23 个 artifacts，未包含 Intel Mac。
- 依赖升级由 `tts-gpu-model-integration` 分支交付，实验由 `codex/moss-nano-candle` 独立保留；不合并实验实现。
