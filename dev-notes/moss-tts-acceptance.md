# MOSS-TTS-Nano 本地验收

日期：2026-10-06。环境：macOS ARM64，Apple M4 Pro。所有模型、音色和录制配置位于 target 下的隔离目录，没有修改真实 ~/.novel 配置。结果为本机验收，不代表 Windows、Intel Mac、Linux 或主观音质已验收。

## 架构与编译

- 新增 novel-tts-backends：MOSS 和 Kokoro 的依赖及模型实现从 core 分离。默认 CLI 仅启用 MOSS；阅读器仍只依赖轻量协议。
- MOSS-only、Kokoro-only、两个后端以及无 TTS 阅读器的 package-specific cargo check 均通过。
- reader dependency tree 不含 core/backends/ort/rodio；默认 novel-tts tree 不含 kokoro-tts。
- workspace all-features 的 lib/tests/examples 测试、Clippy -D warnings、格式检查和严格 rustdoc 通过；文档 Astro 构建通过。
- JSON Lines 升级为 v2，已有配置缺 backend 时仍解释为 Kokoro；新配置为 moss/Weiguo。协议回归验证目录查询和切换不准备模型。

## 真实模型与官方对照

保持 ort = 2.0.0-rc.10，加载 prefill、decode_step、local_fixed_sampled_frame、codec decode_step 和 codec encode，无运行时升级。所有固定资源都通过准备阶段尺寸 / SHA-256 校验。

官方参考来自 OpenMOSS/MOSS-TTS-Nano commit 8b7bcc9341b3b4ef3a3a58ba1338a7d85ff133eb，Python ONNX Runtime 1.19.2，Rust 使用 ORT 1.22。验收工具安装在 target/moss-research/venv，仅用于对照；Python 3.9 的 zip(strict=True) 通过测试脚本兼容层处理，未修改官方算法。

输入「你好，欢迎使用听书功能。」、Weiguo、固定 LCG seed=42、每 3 frame 一次解码：

- SentencePiece tokens 一致。
- 两边生成 45 个 audio frame，172800 个双声道 sample frame（3.6 秒）。
- 全部 PCM 最大绝对差值 1.4454126e-6，RMS 差值 5.203276e-8。
- 已保存 512 个跨完整音频范围的数值探针到后端测试 fixture，容限 1e-4。
- 设置 TRNOVEL_MOSS_MODEL_DIR 后执行真实模型测试，验证数值对照，以及丢弃旧流后新流能正常完成。

补充英文 / Ava 对照：全部 PCM 最大绝对差值 1.0300428e-6。

中英混合输入「今天我们学习 Rust，welcome to the reading session，接下来继续阅读。」也已对照。Python ORT 1.19.2 与 Rust ORT 1.22 在第 30 帧开始出现采样分歧；改用 Python 3.12 + ORT 1.22.0 后，全部 76 帧、291840 个双声道 sample frame（6.08 秒）及全部 PCM 完全一致。分词相同，差异由不同运行时的数值计算影响自回归采样产生。新增混合文本 fixture，避免把不同 ORT 版本的随机生成结果当作逐样本等价要求。

## 生成样例与性能记录

以下为 debug 构建，包含 WAV 写入，不是 release 性能承诺；首块时间从模型加载完成后计算，并包含分段。

| 场景 | 首块 | 输出时长 | 生成耗时 | 最大 RSS |
| --- | --- | --- | --- | --- |
| 英文 / Ava | 169 ms | 3.44 s | 0.81 s | 1.24 GB |
| 中英混合 / Weiguo | 172 ms | 6.08 s | 1.16 s | 1.26 GB |
| 导入音色 | 106 ms | 4.40 s | 0.75 s | 1.20 GB |
| 444 字长文本 / 6 段 | 265 ms | 95.76 s | 16.28 s | 1.48 GB |

使用 CLI 导入生成的 WAV 为 custom:reader，voices list 显示名称，后续合成仅读取缓存。单声道 16kHz 参考音频的 sinc 重采样、静音拒绝、重复 ID、路径越界和旧 revision 均有自动测试。

## 会话及 UI

- 自动测试覆盖有界预算、分块播放、显式结束、流断连、格式变化、失败重试位置、暂停、跳转和旧会话隔离。
- 后端切换期间保护配置更新，防止快速输入把旧后端音色 ID 发给新后端；旧资源事件不能重新标记新后端就绪。
- VHS 真实截图：target/moss-vhs/moss-settings.png、kokoro-selected.png、voice-selected.png、model-ready.png、playing.png、paused.png。
- 实际 TUI 在 MOSS 就绪后播放，绿色高亮对应当前原文片段，暂停显示 Paused。
- 基础阅读版使用独立复制的二进制重新录制，设置 / 帮助不压缩正文，帮助中没有 TTS 行。截图位于 target/moss-vhs/fixed-*.png。

## 复现

```sh
cargo build -p novel-tts -p trnovel --features novel-tts/kokoro
cargo run -p novel-tts-backends --example moss -- target/moss-models/moss target/moss-research/native.wav
TRNOVEL_MOSS_MODEL_DIR="$PWD/target/moss-models/moss" cargo test -p novel-tts-backends
cargo test --locked --all-features --workspace --lib --tests --examples
cargo clippy --all-targets --all-features --workspace -- -D warnings
cargo fmt --all --check
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --document-private-items --all-features --workspace --examples
corepack pnpm@10.32.1 --dir docs build
```

模型文件和音频/GIF 在 target，未加入 Git。可复用的 tape 和测试 fixture 已纳入源码。跨平台实机播放和长时间主观试听仍需相应环境；沿用之前 decouple-tts-process 的发布渠道验收跟踪。

## 连续中文与显示回归修复

- 下载进度改为两行：资源文件名，以及进度条 / 一位小数百分比 / MiB；未知大小和超出总量的进度有回归测试。
- 听书速度、音量固定一位小数；VHS 用 2.6999996 配置验证 UI 显示 2.7x，截图 `target/moss-vhs/fixes-settings.png`。
- MOSS 改为优先按句、50 token / 60 个 CJK 字符分段，避免 BPE token 合并多字导致超过 375 帧。新增句子边界和多字 token 的预算回归。
- 使用用户截图中的连续中文段落真实合成：7 段，43.52 秒音频，首块 181 ms，生成 7.84 秒，全部显式完成；输出及日志位于 `target/moss-research/continuous-fixed.*`。
- 更新后全工作区测试（含真实模型 fixture）、Clippy、格式、严格 rustdoc 和 OpenSpec 校验通过。模型的安全帧数上限仍保留，异常生成不会被伪装成正常结束。

## 连续朗读韵律调整

上一轮逐句合成避免了长段落超限，但每个短句独立启动 / 收尾会强化停顿。现在在 50 token / 60 个 CJK 字符预算内合并相邻句子，预算超限优先切完整句末，其次是逗号；换行仍保留段落边界。

同组中文文本从 7 段降为 4 段，完整生成 48 秒音频（首块 223 ms，生成 7.78 秒），未触发上限。示例 `target/moss-research/continuous-grouped.wav` 可供主观试听；不能以生成完成证明音质改善。句子合并、段落边界及预算回归、真实模型 fixture、全工作区测试、Clippy、格式和严格 rustdoc 均通过。

## 装饰行朗读修复

MOSS 与 Kokoro 的章节分段现在跳过整行至少三个装饰符组成的分隔线；正文运算符和原文坐标保持不变。回归覆盖 200 个等号、CRLF、多字节字符、正文 a=b、减号表达式及空白行。真实 MOSS 输入「分隔线 / 第一章 / 分隔线 / 正文」得到两个可朗读片段、完整 6.24 秒音频，分隔线不会进入推理请求。全工作区测试（含真实模型）、Clippy、格式、严格 rustdoc 和 worker 构建通过。
