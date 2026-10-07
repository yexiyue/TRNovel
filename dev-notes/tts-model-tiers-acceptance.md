# GPU 主力与 CPU 补充 TTS 验收（2026-10-07）

## 实现范围

按 Qwen → VoxCPM2 → OmniVoice → ZipVoice 顺序验证原生推理并接入 worker。发布运行无需 Python；Python 仅用于开发期官方数值对照、ASR 和验收驱动。阅读器仅依赖协议 v5，模型与音频运行库留在 worker。MOSS/Kokoro 保留，新增候选按需 feature。

配置 model 可选；旧 Qwen None 为 0.6B-CustomVoice，已有设备、音色、后端不迁移。新文件有可用 CUDA/Metal 且编入 Qwen 时选择 1.7B-CustomVoice 与检测到的设备，否则 CPU MOSS → Kokoro → Qwen 0.6B。没有稳定后端时须显式选择已编译候选。Auto 仍执行完整链路校准；新 GPU 默认不强制先进行昂贵的 1.7B CPU 校准。模型切换关闭会话并释放旧线程/模型后再加载。

## 固定来源和许可

所有权重清单保存 path、固定 URL、size、SHA-256，下载与每次准备均验证。

| 模型 | revision | 总字节数 | 权重许可 / manifest |
| --- | --- | ---: | --- |
| Qwen 0.6B CustomVoice | 85e237c12c027371202489a0ec509ded67b5e4b5 | 2,498,383,173 | Apache-2.0；src/qwen/resources.json |
| Qwen 1.7B CustomVoice | 0c0e3051f131929182e2c023b9537f8b1c68adfe | 4,520,159,149 | Apache-2.0；src/qwen/models/1.7b-customvoice.json |
| Qwen 1.7B Base | fd4b254389122332181a7c3db7f27e918eec64e3 | 4,544,169,927 | Apache-2.0；src/qwen/models/1.7b-base.json |
| Qwen 1.7B VoiceDesign | 5ecdb67327fd37bb2e042aab12ff7391903235d3 | 4,520,158,662 | Apache-2.0；src/qwen/models/1.7b-voicedesign.json |
| VoxCPM2 Q8_0 + F16 | 169f64d8b98bbaab1761e4ca3a83e6af653456cc | 3,552,406,272 | Apache-2.0；src/voxcpm/resources.json |
| OmniVoice 0.6B | c5fdb5ccb189668d56333f77ba2629f4cd7535f4 | 3,267,454,092 | 生成器 Apache-2.0；tokenizer BOSON/Higgs/Llama；src/omnivoice/resources.json |
| ZipVoice Distill INT8 | 4ed45fb6e7e9527b780bef9e097a04bf13fe4e6b | 184,387,290 | Apache-2.0 / Vocos MIT；src/zipvoice/distill-int8.json |
| ZipVoice Distill FP32 | 同上 | 549,327,724 | Apache-2.0 / Vocos MIT；src/zipvoice/distill-fp32.json |

清单位于 `crates/novel-tts-backends/`；以实际文件为准。代码来源：Qwen TrevorS 711ceee07cad92673f86de8997bdf54c30caa49f（MIT）；Vox tc-mb/llama.cpp-omni 873056743b74e1a4ce5dcf7290e2298428e214db（MIT）；Omni FerrisMind 4c7b088294fd6f3a5fb9e3b1dfd3a77bc12f105b（Apache）；Zip k2-fsa/ZipVoice 2f7326fbfe999a3ad179e3f1af82a424d4a62819（Apache）。各 vendored SOURCE.md 保留修改说明。

Zip 中文词典、数字与多音字流程复现 Emilia/Jieba/Pypinyin/Cn2An；英文独立工具使用 patched eSpeak 0f65aa301e0d6bae5e172cc74197d32a6182200f（GPL-3.0-or-later），对应 source/data/license 随选入该 feature 的发行包附带。不是另一套 ML 运行时。Omni tokenizer 不能概括为 Apache/MIT；原始许可见 crates/omnivoice/LICENSE.Higgs-Audio。

## Windows 真实短测

RTX 5070 12 GB，驱动 616.56，CUDA Toolkit 12.9 Update 1，MSVC 14.44，Rust 1.98，Candle 0.9.2。/MD 与 ORT 共存。GPU 适配器为 release 构建。CPU 同机；部分编译/ASR 任务并发，不能作为跨模型严格排名。

| 模型 / 设备 | 首 PCM ms | 生成 ms | 音频 s | RTF | 证据 WAV / log |
| --- | ---: | ---: | ---: | ---: | --- |
| Qwen 0.6B CUDA | 887 | 6126 | 8.08 | .758 | target/qwen-cuda-06-baseline |
| Qwen 1.7B Custom CUDA（二十帧） | 1378 | 7331 | 10.24 | .716 | target/qwen-cuda-17-chunk20 |
| Qwen 1.7B Base CUDA，VoiceDesign 参考 | 1596 | 6623 | 8.32 | .796 | target/tts-integration/qwen-designed-base |
| Qwen 0.6B CPU | 3791 | 15767 | 3.68 | 4.28 | target/tts-integration/qwen-cpu-06 |
| VoxCPM2 CUDA 克隆 | 455 | 2550 | 9.92 | .257 | target/tts-integration/voxcpm-adapter-clone |
| VoxCPM2 CPU 克隆 | 6106 | 20130 | 2.40 | 8.39 | target/tts-integration/voxcpm-adapter-cpu |
| OmniVoice CUDA 克隆 | 1594 | 1594 | 7.24 | .220 | target/tts-integration/omni-adapter-clone |
| OmniVoice CPU 克隆 | 44869 | 44869 | 1.98 | 22.66 | target/tts-integration/omni-adapter-cpu |
| ZipVoice INT8 CPU，原生 ORT 探针 | 全段 | 5410 | 5.707 | .948 | target/tts-integration/zip-int8 |
| ZipVoice FP32 CPU，原生 ORT 探针 | 全段 | 6860 | 5.707 | 1.202 | target/tts-integration/zip-fp32 |

早期 Zip 探针 Rust wrapper 为 debug、ORT 内核为优化 native；最终 release 适配器指标见下文。Omni/Zip 首 PCM 即整个语义段完成，native_streaming=false。Vox 使用 48kHz 原生 PCM，CPU 确认 n_gpu_layers=0、offload_kqv=false、op_offload=false 及 CUDA compute buffer 0 MiB。GPU linked 程序出现 CUDA_Host pinned host memory 不等于 GPU 计算。

Qwen Base 克隆为渐进流式，并在每帧/解码边界检查取消。设计过程只生成短参考，之后 Base 复用音色提示。Qwen 风格 debug 探针已正常 EOS（qwen-styled.wav），不将该 debug RTF 当作 release 指标。

## 持续播放与取消

Qwen 1.7B CustomVoice release worker 真实音频设备连续播放 1808.75 秒、226 段，Playing 状态成立，无失败；取消后下一请求 Completed，shutdown 正常退出。累计生成 RTF .758（包括初始化），nvidia-smi 整卡显存峰值 6304 MiB，早/晚中位数 6273/6274 MiB，无持续增长。证据 `target/tts-integration/qwen-soak/{result.json,events.jsonl,worker.log}`。

VoxCPM2 release worker 克隆音色连续实际播放 1805.55 秒、323 段，无失败；取消后下一请求 Completed，shutdown 正常。生成累计 RTF 0.266（扣除通道背压），整卡显存峰值 7261 MiB，早/晚中位数 6271/6280 MiB；没有随每段持续增长，整卡采样包含桌面及并行构建。证据 target/tts-integration/voxcpm-soak/。stop acknowledgement 为 0ms，不能当成物理释放耗时。OmniVoice 在用户要求停止播放时结束本轮：已持续 1561.45 秒（约 26 分钟）、330 段，Playing 成立，期间无失败；累计生成 RTF .272，整卡显存峰值 3262 MiB、早/晚中位数 3109.5/3112 MiB，峰值 RSS 2,975,387,648 bytes。证据 omnivoice-soak-final/user-stopped-result.json。未满 30 分钟，保留该验收项；终止 worker 不计作正常 shutdown 或取消/重试通过，后者另有原生探针证据。

取消后丢弃音频 receiver，不再向播放器交付 PCM；Vox 原生回调 false 立即中断，帧上限返回 Truncated，不伪造 End。Omni 扩散及解码直接检查 receiver 是否关闭，避免 Backend Drop join 阻塞 Tokio 时无法更新取消标志。Zip 八步之间和解码边界检查取消。所有后端验证取消后下一请求正常，物理释放耗时与协议 stop acknowledgement 分开记录，不能把 stop ack 当成 GPU 已释放。

## 对照、语料和可重复运行

`tools/tts/acceptance-corpus.txt` 固定标题、对话、数字/日期、多音字、英文混读和长段落。`native_probe` 按生产语义分段导出 WAV，支持 @UTF-8文本文件；取消环境 NOVEL_TTS_PROBE_CANCEL（首 PCM 后）、NOVEL_TTS_PROBE_CANCEL_EARLY（100ms 后）、NOVEL_TTS_PROBE_DROP_EARLY（测取消并同步释放）。配置 NOVEL_TTS_PROBE_STYLE 可测 Qwen 1.7B 风格。

Zip 有 17 组官方完整前端音素对照；Torch mel fixture 误差 < .002，Vocos ISTFT < 1e-6。fixture 生成依赖只用于开发，工具 `tools/tts/generate_zipvoice_frontend.py` 记录上游版本和完整字典来源，不使用简化拼音替换。

开发期 CPU Whisper-small INT8 辅助转写发现日期中的“〇”识别为“先/千/发”，以及“章/掌、归途/龟土”等同音词。自动转写只提供待试听线索；Zip INT8/FP32 的短文内容匹配，Qwen/Vox/Omni 日期与英文混读需逐段人工核对。当前无人逐段确认听感，音质验收保留。

## 构建、隔离和待验收

Windows 全 features workspace 测试、Clippy、rustfmt 和完整 rustdoc 通过（普通测试无大模型自动下载）；裁剪 feature、worker 协议和组件/模型切换结果见下文。旧配置保留、资源续传/校验、无正常 End 不写完成检查点已有回归覆盖。

Linux CUDA、macOS Metal 在专门 CI 工作流加入构建检查；本机没有对应 SDK/OS，本次尚未执行这些 CI，不声称平台构建或实机已验收。历史 M4 Pro Qwen 0.6B 结果在 qwen-tts-acceptance.md 保留，仅适用于当时版本。人工听感及两个外平台实机均未完成。

ZipVoice production release 全语料：11 段，音频 78.88s，首 PCM 6037ms，生成 88427ms，RTF 1.121（并行 CPU 负载）；短文独立采样首 PCM/生成 4872ms、音频 5.707s、RTF .854、峰值 RSS 1,111,953,408 bytes，证据 zip-release-metrics/。FP32 首 PCM/生成 6556ms、RTF 1.149，取消并释放 1203ms。INT8 全语料取消并释放 1190ms；Omni CPU 提前取消、同模型下一请求正常完成，取消并同步释放 2426ms，证据 omni-cancel-cpu.log。

最终 release 全语料（含长段落）：Qwen 1.7B CustomVoice 风格指令，8 段，音频 78.08s，首 PCM 1290ms、生成 55482ms、RTF .711、取消并释放 84ms、峰值 RSS 4,203,577,344 bytes；Vox 设计音色 11 段，音频 53.76s、首 PCM 660ms、生成 14182ms、RTF .264、取消并释放 1069ms、峰值 RSS 3,060,817,920 bytes；Omni 设计音色 11 段，音频 53.29s、首 PCM 2925ms、生成 16067ms、RTF .302、取消并释放 61ms、峰值 RSS 3,026,612,224 bytes。三者提前取消后的同模型下一请求均正常 EOS。证据对应 target/tts-integration/{qwen-style-release-corpus,voxcpm-release-corpus,omni-release-corpus}.wav 和同名指标目录。

Vox/Omni production CLI voices design 成功，保存的设计参考经共享音色库用于上述完整语料克隆。固定语料的阿拉伯日期、Hello world/Enter 转写基本覆盖；“他/她”“轻声/亲生”“行长/航长”“弯曲/湾区”等含同音字或识别疑点，仍需人工逐段试听。Zip 英文 Enter 被识别为字母，不宣称该项音质通过。

Windows 最终全 feature Clippy（warnings deny）、完整 workspace library/bin rustdoc（warnings deny）通过。默认 reader 独立构建通过，cargo tree 确认不包含 Candle、ORT、tts-core/backends、Vox 或 Omni；examples rustdoc、裁剪 feature 和组件/worker 回归亦通过；物理终端按键另留人工验收。

首次实际启用会在锁内保存选择的默认配置；目录查询不写文件，已存在文件初始化不改变原始字节。新增回归覆盖一次保存、重新打开仍保留、旧未知字段原文不变和无效配置不创建文件。

阅读器实际设置组件渲染 + 生产调整回调 + 隔离配置的真实 worker 回归通过：Qwen 0.6B→1.7B 风格→Base 克隆/音色→CustomVoice，后端边界不丢失模型，切换 Zip/Omni 更新音色和 native_streaming。此项不等同于 Windows 物理终端键盘操作；该人工项仍未验收。

最终 Windows 全 workspace library/tests/examples、全 target Clippy、library/bin/examples rustdoc（warnings deny）、格式检查通过；Qwen/Vox/Omni/Zip/Kokoro/MOSS 单后端与 Qwen CUDA、无后端裁剪 Clippy 通过。Astro 文档站 frozen-lockfile 安装与构建通过。主程序原生依赖隔离已独立检查。

逐段试听产物按模型保存到 `target/tts-integration/{qwen,voxcpm,omni,zip}-listening-corpus-segments/`，`index.json` 提供原文、文件名和采样区间，无自动播放。新增 Zip INT8 完整语料独立生成 RTF .877（69,183ms / 78.88s）、首 PCM 3829ms，正常 EOS；语料与前述 ASR 对照一致，仍不代替人工听感确认。

最终逐段导出共 41 个 WAV（Qwen 8，其他各 11），试听入口 `target/tts-integration/listen.html`，不自动播放。该轮正常 EOS：Qwen 风格 RTF .772 / 首 PCM 1523ms；Vox 设计音色 .275 / 601ms；Omni 设计音色 .319 / 3477ms；Zip INT8 .877 / 3829ms。同一固定文本采用各后端生产分段策略；并行编译时的指标不用于严格质量或速度排名。

最终 release 单进程静音播放与切换通过：Qwen 0.6B→1.7B CustomVoice→Base→VoxCPM2→OmniVoice→Zip INT8，每个模型正常 Completed，shutdown 为 0。在 ConfigChanged 后、下一 Prepare 前采样，旧模型释放后整卡显存约 1688–1776 MiB，未保留两套权重；Zip CPU 准备前后显存不增加。证据 model-switch-retry/result.json / events.jsonl / worker.log。

## OpenSpec 对照复核

使用 openspec-verify-change，对四个 spec-driven 变更读取 proposal/specs/design/tasks，并执行 strict validate，四者均合法。

| 变更 | 完整性（任务） | 正确性（ADDED 要求） | 架构一致性 |
| --- | --- | --- | --- |
| qwen-model-tiers | 8/10；剩余验收未完成 | 2/2 实现有代码和运行证据；外平台场景未实机验证 | 原生 worker、统一运行库、显式设备、PCM/End 和线程所有权符合设计 |
| integrate-voxcpm2 | 8/9；剩余验收未完成 | 1/1 实现有代码和运行证据；外平台场景未实机验证 | 原生 worker、统一运行库、显式设备、PCM/End 和线程所有权符合设计 |
| integrate-omnivoice | 8/10；剩余验收未完成 | 1/1 实现有代码和运行证据；外平台场景未实机验证 | 原生 worker、统一运行库、显式设备、PCM/End 和线程所有权符合设计 |
| integrate-zipvoice | 7/8；剩余验收未完成 | 1/1 实现有代码和运行证据；外平台场景未实机验证 | 原生 worker、统一运行库、显式设备、PCM/End 和线程所有权符合设计 |

实现映射：Qwen 默认与克隆要求对应 `crates/novel-tts-backends/src/lib.rs:206`、`crates/novel-tts-core/src/config.rs:65`、`crates/novel-tts-backends/src/qwen.rs:197` 和 `crates/novel-tts-backends/src/qwen/design.rs:7`。Vox/Omni/Zip 的有效 PCM、正常 End、取消和显式设备要求分别对应各自 runtime.rs，以及 core `session/producer.rs`、`session/playback.rs` 和准备设备校验。取消/检查点回归见 core session.rs 的 `failures_during_prebuffering_preserve_unplayed_source`、`synthesis_does_not_advance_playback_and_cancel_has_one_terminal`；真实模型由 native_probe 与协议切换/长播放证据覆盖。非法模型、音色身份和参考输入另由 model/config/voices/reference 测试覆盖。

CRITICAL（归档完整性）：共六个未勾选的组合验收任务，涉及物理终端键盘、逐段人工听感、Linux CUDA/macOS Metal 和 Omni 满 30 分钟。补齐对应条件后再归档；此次不归档、不把未验收记为通过。WARNING：编译矩阵和源代码检查不能代替每个平台的实际推理/体验验收。未发现已执行检查中的功能或架构偏离，外平台场景与人工质量未验证，无新增代码风格建议。

新 GPU 用户 release 实测 Qwen 1.7B/CUDA：Hello/GetConfig 不写文件，首次 PrepareModel 保存并准备成功；首次保存与查询快照一致。CPU-only worker 新用户查询选择 MOSS/CPU，未创建配置或模型。证据 first-activation-final/result.json、cpu-default-final/result.json。

最后复核修正初始化取消的所有权：四个新适配器先构造持有请求通道与 JoinHandle 的后端，再 await ready；初始化 future 被取消时关闭通道并 join，避免 thread handle 被直接丢弃而脱离所有者。新 NOVEL_TTS_PROBE_CANCEL_LOAD 实测在 100ms 请求取消，完成释放后再加载同模型；Qwen/Vox/Omni/Zip 均真正命中取消并在下一请求正常 PCM/EOS。包含等待不可中断初始化完成的物理耗时分别 5009/2623/434/2255ms，区别于已加载模型的生成取消耗时。证据 cancel-load-{qwen,voxcpm,omnivoice,zipvoice}/stdout.log、metrics.json 和 WAV。

最终修正后 debug 阅读器/worker、release worker 构建通过；debug `novel-tts --help` 正常启动。全 workspace 测试、Clippy、examples rustdoc、格式再次通过。当前没有播放 worker；验收未改变真实用户配置，测试配置与声音保存在 target/tts-integration。

## 2026-10-07 用户试听反馈与 MOSS 扩展

用户确认 OmniVoice 是句中漏字，听感不如 ZipVoice Distill FP32，VoxCPM2 2B 最好。该反馈不被既有 EOS/吞吐验收覆盖，中文完整性和音质仍独立判断。按用户新要求增加统一 Candle 的 MOSS Local / Realtime / VoiceGenerator，详见 `moss-candle-acceptance.md`；不改变当前用户的后端、模型、音色或设备。

## macOS Metal 补测

M4 Pro 的六种模型真实 PCM、同机 CPU 对照、较长语料和 Mac 构建/设备识别修复见 [Metal 效率报告](metal-tts-efficiency.md)。各模型可完成推理，但本轮预热后 RTF 均大于 1；不将可用性写成实时连续播放验收。权重已放默认缓存，人工音质、30 分钟播放及其他平台的组合任务仍保持未完成。
