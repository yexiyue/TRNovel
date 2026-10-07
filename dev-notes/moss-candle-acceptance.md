# MOSS Candle trial acceptance — 2026-10-07

用户反馈：OmniVoice 句中漏字，听感不如 ZipVoice Distill FP32，目前 VoxCPM2 2B 最好。本轮增加 MOSS 候选供试听，不改变已有选择，不以 EOS 或自动转写代替逐字人工验收。

## Resources and runtime

`crates/moss-tts` 使用工作区 Candle 0.9.2。Local、Realtime、Delay 的调度共享 Transformer、采样和 MOSS Audio Tokenizer。发布推理不调用 Python。Nano 保留 ONNX、原目录和原音色格式。

| 资源 | 固定 HF revision | 全部资源字节数 |
| --- | --- | ---: |
| Local 1.7B | `12aa734e4f11a7b3fdf4eb0ad2aa2029675ffc2e` | 6,132,763,299 |
| Realtime 1.7B | `75682787d8e2fcc73faca37ba2931453ca9c4022` | 4,675,362,524 |
| VoiceGenerator | `97521ec2b6f3ec5026ac1f5751f8fc302d82c2d4` | 4,239,709,267 |
| Audio Tokenizer | `3cd226ba2947efa357ef453bcad111b6eafba782` | 7,098,616,442 |

URL、尺寸和 SHA-256 见 `crates/moss-tts/assets/*.json`，已逐文件下载校验。四个官方 model card 均声明 Apache-2.0，源码许可见 LICENSE/NOTICE。TTS 源码 commit `934d6826b084c46a0d033402174d5f8ac4ed2519`；codec commit `8c50ac4c5d7287d2ed6ea20a08c90ca439887d23`。

权重放在 `C:/Users/Administrator/.novel-tts/moss/models/<id>/<revision>`。各模式只共用一次 codec 下载，不复制 7.1 GB codec。1.7B 指文本骨干，不能用它估算完整 checkpoint/显存。

## Numerical checks

- 固定官方小模型夹具覆盖 Qwen3 全局、无 RoPE 的 Local 深度 Transformer、codec 完整/分块/跨窗口；CPU F32 误差上限 `1e-5`，三个测试通过。
- Local prompt 与官方 UserMessage 和固定 chat_template.jinja 一致。
- 初轮长音频对照发现 ring-cache 在注意力前覆盖旧 key；已修正并增加跨窗口回归。修正后的 Realtime 克隆样本对官方 F32：282,240 samples，RMSE `.0003135`，相关系数 `.9999988`。
- 同参考 F16 编码对官方 F32，第一个码本一致率 `.98958`，后续 `.875–.98958`。量化边界受精度影响，不宣称 RVQ codes 逐项一致。证据 `moss-realtime-designed.encoder-comparison.json`。
- VoiceGenerator 的 F16 真实生成出现无效 logits；CUDA 模型改用官方 BF16、codec 使用 F16 后正常生成。设计目前仅开放已验证的 CUDA/BF16。

## RTX 5070 evidence

Windows / RTX 5070 12 GB / CUDA 12.9 / release。各样本用途不同，不作为统一音质排名。

| 模式与样本 | 首 PCM | RTF | 结果 |
| --- | ---: | ---: | --- |
| Local F16，中文/数字/日期/英文混读 | 1.128s | .981 | 27.36s WAV、正常 EOS；未达主力 .8 门槛 |
| Local BF16，实际 worker adapter | .419s | 1.029 | 13.2s WAV、正常 End；转写提示开头短句可能漏读 |
| Realtime F16，同一混读文本 | .823s | .619 | 29.2s WAV、正常 EOS |
| Realtime BF16，实际 worker adapter | .256s | .633 | 4 个语义段、14.08s WAV、正常 End |
| VoiceGenerator BF16 描述音色 | .631s | .307 | 7.68s 参考 WAV，取消后再次生成成功 |
| Realtime 复用设计参考 | .596s | .630 | 新小说段落 11.76s WAV、正常 EOS，不重新设计 |
| Realtime BF16，复用 CLI 保存音色 | .300s | .699 | 6.32s WAV、取消后再次生成正常 End |

初轮整卡峰值显存（含桌面）：Local 10,925 MiB、Realtime 9,609 MiB。Realtime adapter 峰值 RSS 6,397,804,544 bytes。初始化取消并释放耗时 3,696ms；已加载生成的早期取消/首 PCM 后取消由同实例的下一次正常 End 验证。初始化不可中断加载耗时与生成取消耗时分开记录。

初轮混读转写基本覆盖数字、日期和英文，但存在“第一章/一张”“轻声/亲生”“她/他”等差异。Local BF16 adapter 的输入以“他轻声问道”开始，Whisper small 对整段和独立前 5 秒都从“明天还会见面吗”开始，保留潜在漏读项，不据此判断是模型遗漏还是 ASR 遗漏。证据 `moss-local-adapter-segments/index.json`、`moss-adapter-asr.log`、`moss-local-opening-asr.log` 与 `moss-local-adapter.wav`。人工逐段听感与 Linux CUDA/macOS Metal 实际推理仍保留独立验收项。

## Product boundaries

`moss-candle-cuda` 添加 Local/Realtime GPU 实验选项。`moss-candle-metal` 的 M4 Pro 真实 PCM/吞吐补测见 [Metal 效率报告](metal-tts-efficiency.md)，不替代 30 分钟持续播放与人工音质验收。VoiceGenerator 是 voices design 辅助模型，不逐段重新设计。Local 在目录注明实验、较慢，不作为默认主力。

CPU 库构建/小模型数值测试通过，完整大模型 CPU 内存/吞吐未验收，worker 暂只公开 GPU 设备，Nano CPU 不变。Auto 只从该模型公开设备选择；有 CPU 对照时保留实测校准，GPU-only 模式不加载未开放的 CPU adapter。显式 CPU 请求报错。

新 feature 的 Nano 目录项有显式 ID nano；旧 model=None 仍匹配 Nano、不自动改写。该 ID 使同后端切回 Nano 可以表达。参考复用现有 model-scoped VoiceStore；编码缓存检查 codec revision、WAV SHA-256、码本数、帧数和 token 范围。参考暂限 1–10s，此限制不作用于 Nano。

试听与指标在 `target/tts-integration/moss-*`：local-corpus、local-adapter、realtime-corpus、designed-first、realtime-designed、realtime-adapter、realtime-saved-voice 的 WAV 与对应日志。未完成项不勾选、不归档。

## Worker, reader and regression

- 实际 CLI `voices design` 已在 Realtime 模型目录保存 `custom:candle_trial`（MOSS 温暖女声试听）；用该音色生成新段落，不重新运行 VoiceGenerator。同一参考另行导入 Local 模型身份，缓存互相隔离，Nano 格式未修改。
- 五次实际静音播放：Nano CPU → Local CUDA → Realtime CUDA → Nano CPU → VoxCPM2 CUDA，全部正常 Completed。切换释放后整卡显存约 1,669–1,706 MiB，加载 Local/Realtime/Vox 分别约 10,862/9,520/6,504 MiB。证据 `moss-model-switch/result.json` 和事件日志。
- 渲染真实设置组件的 TUI 回归通过：Nano CPU → Local Auto → Realtime → 保存音色 → Local → Nano；不兼容的旧设备切换为 Auto，音色随模型重置。显式请求 Realtime CPU 则报错，不下载、不写配置，证据 `moss-invalid-device.log`。
- release 和 debug worker 编入 `qwen-cuda,voxcpm-cuda,omnivoice-cuda,zipvoice,moss-candle-cuda`，debug 阅读器构建通过。阅读器普通依赖树不包含 Candle、ORT 或推理后端。
- 完整工作区测试、all-targets/all-features Clippy `-D warnings`、fmt、examples 及 lib/bin rustdoc `-D warnings` 全通过。裁剪 feature（无后端、moss、moss-candle、CUDA、Metal 入口）Clippy 通过；在 Windows 编译 Metal 入口只验证门控，不等价于 macOS 构建。OpenSpec strict validate 与开发 Python 工具语法检查通过。
- 文档站 `pnpm build` 通过（19 页）；本轮不修改系统环境变量、PowerShell profile 或用户原有听书配置。
- Realtime BF16 / `custom:candle_trial` 连续实际播放 1,800 秒通过：启动 321 个片段，其中 320 个播完、最后一个主动取消；零缓冲欠载。整卡显存 9,523–9,528 MiB（开始 9,523、最后 9,525），未持续增长。随后同实例的新请求正常 Completed，整个测试含复用约 1,807.844 秒，worker 正常退出。
- 停止确认在同一 Windows monotonic 时钟刻度内返回，脚本记录 0ms；只说明小于本次计时分辨率，不能解释为精确零延迟。初始化取消的耗时仍单独记录。已加载生成还验证了首 PCM 后关闭通道取消及再次生成。
- 持续播放使用独立配置，将音量设为零后运行真实播放时钟，不修改用户配置。证据 `moss-realtime-soak/result.json`、`events.jsonl`、`worker.log`；Local 尚未进行 30 分钟持续播放。人工音质和逐字覆盖未确认，不宣称 MOSS 优于用户当前偏好的 VoxCPM2。
