# VoxCPM2 Candle 移植验收（2026-10-07）

## 当前结论

`crates/voxcpm` 已实现独立 Candle 计算链，并使用现有 GGUF 实际输出 48kHz PCM。
**用户已授权先接入，生产适配器已切换到 Candle，接入回归已通过。完整迁移验收仍未通过。**
严格 F32 对照仍失败；用户接受该差异先接入，不放宽阈值。C++ 暂留开发基准，人工听感及其他未验收项保持未完成。
OpenSpec：`openspec/changes/voxcpm-candle-migration/`。

## 可复现来源和运行方式

模型语义：OpenBMB/VoxCPM `f0c787f0937dc1c9a8f4f64d9a332d9c5da2e629`（Apache-2.0）。
GGUF 映射及生产对照：llama.cpp-omni `873056743b74e1a4ce5dcf7290e2298428e214db`（MIT）。
资源 revision：`169f64d8b98bbaab1761e4ca3a83e6af653456cc`。
文件大小、SHA-256、许可和默认目录见 `crates/voxcpm/SOURCE.md`；没有转换或重下权重。
推理只使用工作区 Candle，初始 0.9.2，当前统一升级到 0.11.0；Python 仅用于开发侧官方数值对照和转写。

运行命令见 `crates/voxcpm/README.md`。对照工具：

- `tools/tts/voxcpm_reference.py`：导入固定官方源码，用同一 GGUF 解量化后的 F32 权重产生逐模块张量。
- `tools/tts/voxcpm_tokenizer_reference.py`：以固定官方中文 token wrapper 复核 SentencePiece ID 夹具，六组 ID 未变化。
- `voxcpm` 的 ignored tests：本地权重数值对照，不在普通 CI 下载模型。
- `voxcpm_candle_probe`：复用生产参考音频重采样和 180 字节分段，预热一次、默认五轮，逐段保存 WAV/原文。
- `tools/tts/probe_metrics.py`：子进程 RSS、整卡显存采样。
- `tools/tts/voxcpm_transcribe.py`：Whisper small/int8 CPU 转写与字符差异，保留简繁、数字差异供人工核查。

本机 RTX 5070 12GB、Windows x64 MSVC、驱动 616.56。下列初始移植数据
使用 Candle 0.9.2 / CUDA 12.9.86；当前 0.11.0 / CUDA 13 数据见最后补记。
Release，seed 42，CFM 10 步，CFG 2.0，温度 1.0，上限 200。
基准串行执行，不与构建或其他推理同时运行。模块 profiling 带 CUDA 同步，须另行测量，不能混入吞吐基准。

## 已有基线

移植前 release 程序复制到 `target/tts-integration/voxcpm-candle-baseline/`：

| 程序 | SHA-256 |
| --- | --- |
| native_probe.exe | 10a602b8d5b696fb03db6730c8e4808aa4a9f1b2edcb6596397c57feb723a10c |
| novel-tts.exe | 66e4e5a7891c6700a6463d22888616af69a569e08f37564629032d1f30cedcb4 |

同一 `tools/tts/acceptance-corpus.txt`、参考 WAV/文字和生产分段：11 段、53.76 秒音频。
加载 2456ms、首 PCM 595ms、生成 14501ms、RTF 0.269736。
此旧实现结果是单次冷运行，**不是五轮热模型对照**，不能计算严格配对的性能变化。

随后新增 `voxcpm_native_benchmark` 补测原生热模型；相同语料/参考/参数，每轮 53.76 秒音频、11 段正常结束：

| 热轮 | 首 PCM ms | 生成 ms | RTF |
| --- | ---: | ---: | ---: |
| 1 | 234 | 13681 | 0.254492 |
| 2 | 235 | 13738 | 0.255552 |
| 3 | 233 | 13611 | 0.253193 |
| 4 | 237 | 13684 | 0.254550 |
| 5 | 236 | 13717 | 0.255158 |

原生热模型平均 RTF 约 0.2546。Candle 的约 0.5261 是其约 2.07 倍，但低于约定门槛。
不同随机数实现和数值路径使实际音频长度略有不同；RTF 对实际输出时长计算，没有强行对齐 WAV 长度。
原生材料：`target/tts-integration/voxcpm-native-hot/` 和 `voxcpm-native-hot-metrics/`。

## Candle CUDA 吞吐

初次无参考生成 RTF 0.845。缓存 RoPE 并采用标准 Candle fused RMS 后同文本 RTF 0.570。
五轮完整语料（带参考文字续接）均正常 EOS：

| 轮次 | 首 PCM ms | 生成 ms | 音频秒 | RTF |
| --- | ---: | ---: | ---: | ---: |
| 预热 | 184 | 30861 | 54.72 | 0.563991 |
| 1 | 100 | 30634 | 54.72 | 0.559845 |
| 2 | 108 | 29925 | 54.72 | 0.546889 |
| 3 | 96 | 28930 | 54.72 | 0.528699 |
| 4 | 97 | 28583 | 54.72 | 0.522362 |
| 5 | 98 | 28240 | 54.72 | 0.516091 |

热轮平均 RTF 0.534777，满足该语料吞吐门槛。加载 4288ms（不含 device 初始化），参考编码/重采样 217ms。
首 PCM 后回调触发取消到返回 0ms，下一请求 EOS，模型/参考释放加 CUDA 同步 25ms。
此取消测试在 PCM 回调内触发，不能代替外部线程在任意 GPU kernel 执行期间的取消延迟测量。
峰值子进程 RSS 932638720 字节；整卡显存峰值 6370MiB，包含桌面等其他占用，不能当成进程独占显存。

原始材料：`target/tts-integration/voxcpm-candle-corpus/round-{0..5}/`（WAV、index.json、summary.json），
`target/tts-integration/voxcpm-candle-metrics/`（stdout.log、metrics.json）。
这批数据在新增元数据校验、输入回归和计时字段之前采集；最终二进制复测记录补充在下方。

## 严格数值对照：未通过

F32 固定输入对照：`atol=1e-5, rtol=1e-4`，未放宽阈值。

| 项目 | 最大绝对误差 | 超容差输出数 |
| --- | ---: | ---: |
| BaseLM 解量化 F32 对官方 | 0.000164986 | 20/6144 |
| BaseLM 完整对增量 | 0.000186920 | 10/6144 |
| ResidualLM | 0.000015259 | 0/6144 |
| Local Transformer | 0.000017166 | 1/3072 |
| LocalDiT Transformer | 0.000020981 | 0/22528 |
| LocalEncoder 投影 | 0.000036240 | 0/4096 |
| FSQ | 0.000000238 | 0 |
| DiT 输入构造 | 0.000030518 | 0 |
| DiT velocity | 0.000002868 | 0 |
| 固定噪声 CFM | 0.000002384 | 0 |
| AudioVAE 编码 | 0.000005484 | 0/512 |
| AudioVAE 解码 | 0.000000072 | 0/15360 |
| AudioVAE 整段对分块 | 0.000000043 | 0/15360 |
| AudioVAE 状态重置 | 0 | 0 |

共 31 个输出超容差，阻碍集中在 Transformer。尚未证明具体误差来源，不能认定只是浮点舍入。
尝试改变 CPU RMS 归约顺序不能解决，候选仍采用标准 Candle RMS。
已定位并修复一个独立错误：非连续三维输入直接交给 Linear 导致 CFG 分支错误；统一 contiguous+二维矩阵运算后固定噪声 CFM 通过。
六组真实 tokenizer ID 与 SentencePiece 对照通过，包含中文、数字、英文、特殊符号；不代表无限制文本已全部覆盖。

Q8/F16 单独报告，不使用上述容差判为通过：

| 路径 | 最大绝对误差 | RMS |
| --- | ---: | ---: |
| CPU Q8 Base 对解量化 F32 | 2.685598 | 0.157699 |
| CUDA Q8 Base 对解量化 F32 | 2.555819 | 0.152076 |
| CUDA F16 CFM 对官方 F32 | 0.002862 | 0.000594 |
| CUDA F16 VAE 对官方 F32 | 0.000107 | 0.000026 |
| CUDA F16 VAE 分块对整段 | 0.000146 | 0.000027 |

原始日志：`target/tts-integration/voxcpm-candle-reference/rust-f32.log` 和 `rust-quantized-f16.log`。
最终 Release 数值复测 `rust-f32-final-release.log` 同样为 31 个超容差输出；tokenizer/CPU Q8 报告测试通过，严格 F32 测试失败。
Debug 数值复测在 CFM 后因未优化 GEMM 太慢而停止，不作为完整验收结果。
全 WAV 不按随机 seed 作位级比较；Rust 噪声发生器与 Torch RNG 不同。

## 尚未完成的门槛

- 严格 F32 Transformer 回归。
- 生产 worker、版本化参考缓存、音色设计保存/复用和 TUI 切换迁移；旧 features.json 保留且候选不消费。
- 30 分钟实际播放、零欠载及持续显存稳定性。
- 人工逐段听感确认；自动转写只能辅助，不能证明音色或音质。
- Linux CUDA、macOS Metal：本机缺相应构建环境/实机，未验收。Windows feature 门控不作为替代。
- 因迁移门槛未通过，未删除 voxcpm-sys、C ABI、CMake 或调整生产打包许可。

## 最终构建复测及质量检查

最终加载/输入校验版本复测（后续仅补充 tokenizer 取消检查和开发工具命名）：

| 热轮 | 首 PCM ms | 生成 ms | RTF |
| --- | ---: | ---: | ---: |
| 1 | 114 | 28148 | 0.514416 |
| 2 | 107 | 30607 | 0.559341 |
| 3 | 101 | 28376 | 0.518574 |
| 4 | 96 | 28159 | 0.514607 |
| 5 | 99 | 28645 | 0.523486 |

每轮 54.72 秒音频、11 段，全正常 EOS。冷加载 3717ms = device 初始化 190ms + 权重加载 3527ms；参考编码 212ms。
首 PCM 后取消 0ms，释放 26ms；峰值 RSS 933703680 字节。
本轮整卡显存峰值 6210MiB；单独长段落 5956MiB；原生五轮 7588MiB，均包含桌面等其他占用，不视为进程显存测量。
空文本、非法采样参数、帧数截断、预填充/CFM 内取消且无 PCM、取消后再生成、初始化取消均通过。
材料：`target/tts-integration/voxcpm-candle-final/` 和 `voxcpm-candle-final-metrics/`。

CPU 最终输入校验版本实际无参考生成：5.76 秒音频，预热 48732ms/RTF 8.460575，热轮 50918ms/RTF 8.840088，首 PCM 6873ms。
正常 EOS，相同取消/错误/截断回归通过，加载 4822ms、释放 287ms、峰值 RSS 5086351360 字节。
材料：`target/tts-integration/voxcpm-candle-cpu/` 和 `voxcpm-candle-cpu-metrics/`。CPU 不适合实时听书，无 CPU 实时吞吐承诺。

工作区质量检查：`cargo test --locked --all-features --workspace --lib --tests --examples -j 2` 通过（530 passed，真实权重 oracle 默认 ignored，单独执行见上方失败记录）。
`cargo clippy --all-targets --all-features --workspace -- -D warnings`、`cargo fmt --all --check`、`git diff --check` 通过。
`RUSTDOCFLAGS=-D warnings cargo doc --no-deps --document-private-items --all-features --workspace` 分别选择 examples、lib、bins 全通过。
新示例改名为 `voxcpm_probe`，避免与 voxcpm-sys 的 `probe` rustdoc 输出冲突；lib/bin 分开执行以避免根包同名输出冲突。
`cargo tree -p trnovel --edges normal` 确认阅读器没有 Candle/ORT/Vox 原生推理依赖。
日志：`target/tts-integration/voxcpm-candle-{workspace-tests,clippy,rustdoc,rustdoc-lib,rustdoc-bin,reader-dependencies}.log`。
独立 CPU crate all-target check、无后端 worker check、仅 voxcpm CPU worker check 已通过。
`cargo build -p novel-tts --all-features` 和 `cargo build -p trnovel --bins` 通过。全 feature worker 联合 CUDA/ORT 链接正常，保持动态 CRT /MD 约定；新 Candle 候选和 ORT 的生产会话共存/TUI 切换尚未验收。
构建日志：`voxcpm-candle-{cpu-check,empty-worker-check,voxcpm-worker-check,worker-build,reader-build}.log`。
OpenSpec strict validate、开发 Python 工具语法编译通过。
Linux CUDA/macOS Metal 未构建：已安装 Rust target 无 Linux GNU/macOS，WSL 仅 docker-desktop，没有对应 CUDA/Rust 开发发行版或 Apple SDK。未把 Windows feature 门控检查计为这些平台通过。

最终 tokenizer 取消检查版本的独立长段落：沿用生产 180 字节语义分段，共 6 段、30.4 秒音频；不是将长文本绕过分段直接生成。

| 热轮 | 首 PCM ms | 生成 ms | RTF |
| --- | ---: | ---: | ---: |
| 1 | 116 | 16163 | 0.531680 |
| 2 | 111 | 16223 | 0.533672 |
| 3 | 110 | 16070 | 0.528648 |
| 4 | 104 | 16196 | 0.532786 |
| 5 | 105 | 16261 | 0.534925 |

全部正常 EOS，取消/截断/非法输入/初始化取消和再生成回归均通过。峰值 RSS 931192832 字节。
材料：`target/tts-integration/voxcpm-candle-long/` 和 `voxcpm-candle-long-metrics/`。
对应开发程序 SHA-256 见 `voxcpm-candle-final/binaries.json`。

CUDA 音色设计短参考（同一模型 `(A deep, calm male voice.)` 前缀）实际生成 7.2 秒 PCM，EOS；随后以此 WAV 验证无参考文字的克隆路径。
设计侧同步 profiling 热轮：prefill 30.6ms、CFM 2379.4ms、decoder 509.0ms、LocalEncoder 248.0ms、BaseLM/FSQ 538.9ms、ResidualLM 102.5ms。
CFM 约占该请求生成时间 62%，是当前主要成本；未降低 CFM 步数来达到吞吐门槛。
profiling 结果只用于成本分析，不计入五轮吞吐验收。材料：`voxcpm-candle-design/`、`voxcpm-candle-design.log`。

该短参考 WAV 经生产重采样器编码到参考前缀，无参考文字克隆生成 6.72 秒 PCM、正常 EOS。
热轮首 PCM 107ms、生成 3551ms、RTF 0.528489、参考编码 182ms，取消及再生成回归通过。
材料：`voxcpm-candle-clone/` 和 `voxcpm-candle-clone.log`。这些结果证明生成路径能运行，不证明克隆音色相似度或人工听感已通过。

Whisper small/int8 CPU 转写已保存到 `voxcpm-candle-final/round-5/transcription.json`、`voxcpm-candle-clone/round-1/transcription.json` 和 `voxcpm-candle-cpu/round-1/transcription.json`。
完整语料的主要句子均有对应转写；存在“石阶→时间”、以及同音字、简繁和数字形式差异，需要人工试听核查，不能据 ASR 宣称没有漏读或音质通过。
克隆和 CPU 短句转写都覆盖了原文。试听 WAV 与原文 index.json 保存在相同目录；所有人工听感项仍未确认。
额外纳入完整语料的 11 个语义段，现在 tokenizer fixture 共 17 组；最终 Release ignored tokenizer 测试通过。

最终官方 token 拆分规则版本再次 release 构建并完成 CUDA 短句 smoke：5.76 秒 PCM、正常 EOS，热轮首 PCM 91ms、RTF 0.522562，取消/再生成/截断/错误输入回归通过。
材料：`voxcpm-candle-final-smoke/`、`voxcpm-candle-final-smoke.log`，此处另存最终开发程序及 `binary.json` SHA-256。
`voxcpm-candle-final/binaries.json` 保留之前五轮/长段落测量时的历史程序摘要，不对应随后重建覆盖的 target/release 程序。
所有结果仍不足以通过严格数值、人工听感和 30 分钟生产播放门槛，生产实现保持不变。


## 用户授权先接入（2026-10-07）

用户明确表示当前数值差异影响不大，要求先接入生产。该决定覆盖原先的切换前置门槛，但不修改 `atol=1e-5/rtol=1e-4`，31 项超差仍记录为数值未通过；人工听感及外部平台也不因此通过。

生产适配器改为 Candle，使用相同 GGUF/目录/后端及模型 ID，不重复下载权重。既有 voice.json、WAV、参考文字和旧 features.json 均保留；旧 features.json 不读取，新 candle-reference-v1.json 验证实现、模型/revision、权重清单摘要、参考 WAV 摘要、文字和 F32/F16 精度。每个推理线程最多缓存一个音色。Auto 校准身份增加 candle-v1，避免沿用旧引擎速度。设计线程在取消/完成时关闭接收端并 join，模型加载也可取消。生产 worker 普通依赖树已确认不含 voxcpm-sys；C++ 仅保留为独立 voxcpm-sys crate 的开发对照基准，待最终验收再删除源码。

性能比较来自同机 RTX 5070、相同语料、同样参数的五轮 release 热测：原生 llama.cpp-omni 平均 RTF 0.254589；Candle 平均 0.526085。生成时间约 2.07 倍，吞吐约 48%；每 100 秒音频分别约需 25.5 秒和 52.6 秒生成。Candle 热首 PCM 96..114ms，原生 233..237ms。Candle 达到既定 GPU RTF<=0.8 的吞吐目标。

**官方 PyTorch GPU 实现尚未做同机性能测试**。当前开发 PyTorch 为 CPU-only；上述原生基准不能标记成官方 PyTorch 速度，也不能据此推算与其的差距。CFM 同步分模块测量约占请求耗时 62%，是后续优化重点。


### 接入后回归

工作区测试 533 passed / 0 failed / 5 ignored；被 ignored 的真实模型数值门槛结果仍以此前单独执行的失败记录为准。Clippy -D warnings、fmt、rustdoc examples/lib/bins、CPU-only 与无后端 worker check、all-feature debug/release worker 及阅读器构建通过。日志 `target/tts-integration/voxcpm-integrated-*.log`。原生 benchmark 移到 `crates/voxcpm-sys/examples/voxcpm_native_benchmark.rs`；生产适配器和其 feature 完全不依赖 C++。发行许可改为 Candle crate 的 SOURCE/Apache-2.0。

实际生产适配器 CUDA 导出完整 11 段语料、54.72 秒 WAV，全部 EOS，生成 28343ms、RTF 0.517964、热首 PCM 110ms、加载 3848ms。加载中取消及释放 221ms，首 PCM 后取消/早取消后下一请求正常 EOS，取消加物理释放 32ms。该轮是接入 smoke，不替代此前五轮统计。材料 `target/tts-integration/voxcpm-candle-integrated/corpus.wav`、`corpus-segments/index.json` 和 `gpu-probe.log`。

既有 voice.json、reference.wav、features.json SHA-256 核对不变。首次从 WAV 编码产生 candle-reference-v1.json，GPU 精度标记 f16；只有新缓存文件被创建。


两分钟实际 rodio 播放：26 个片段、零缓冲欠载，停止后下一请求正常 completed。预热后采样 RSS 945348608..948224000 字节、整卡显存 6005..6023MiB（含桌面等其他占用，不等同进程显存）；停止 ack 在本机计时粒度内为 0ms。两分钟不能代替 30 分钟门槛。日志 `voxcpm-candle-integrated/playback/`。

同一个 all-feature worker 实际完成 Vox Candle GPU → ZipVoice FP32 ORT CPU → Vox Candle GPU，并完成 Vox 音色切换。模型切换时整卡显存回落到 1852..1894MiB 后才加载新的 Vox（5900..5929MiB）；同模型换音色复用既有权重，未要求卸载后重载。ORT CPU 和 Candle CUDA 同进程共存通过；不将此记为 ORT CUDA provider 的额外实机验收。日志 `voxcpm-candle-integrated-switch/`。渲染 TUI 内的实际按键切换仍未执行。

生产适配器 11 段 WAV 与之前独立 Candle 五轮的 round-5 WAV **逐字节完全一致**，包含 WAV 格式/PCM，结果 `candidate-comparison.json`。因此原有该轮 ASR 材料仍对应接入后的相同音频，人工听感和其中识别差异依然待确认。

真实 CPU 带文字参考音色续接输出 2.56 秒有效 PCM、正常 EOS；加载 4686ms，首 PCM 26835ms，生成 44559ms / RTF 17.405859（含首次 CPU 参考编码与预填充，冷短请求，非持续热吞吐）。比无参考路径更慢，CPU 不适合该模型的实时听书。保留 CPU 功能，不宣称达到 GPU 或 CPU 实时门槛。日志 `cpu-probe.log` 与 `cpu-continuation.wav`。

人为设置新缓存为不兼容实现标记，真实 CUDA 适配器成功从原 WAV 重建并恢复 voxcpm-candle-v1/f16，随后正常 EOS。CLI 创建短参考设计音色，再通过生产适配器克隆生成 4 秒有效 PCM、正常 EOS；该临时验收音色随后用 CLI remove 清理，只保留试听材料在 target，未改变用户配置或原音色。材料 `design-reference.wav`、`design-clone.wav`、`designed-voice.json`、`cache-rebuilt.wav`。

未完成：严格 F32 对照（用户已接受当前差异先接入）、30 分钟连续播放、人工听感确认、实际 TUI 按键切换、Linux CUDA/macOS Metal 对应环境构建与实机、CPU 无文字参考克隆和设计的完整模式矩阵。C++ 独立开发基准 crate 尚未删除；生产依赖和 feature 已完全移除它。OpenSpec 不把这些项勾为完成。

## 原始 Safetensors 与 CFM 缓存对照（2026-10-07）

原始模型固定到 OpenBMB/VoxCPM2 revision
`32279effe8c19989596f05d353d1447f51d9e915`，SHA/尺寸见
`crates/voxcpm/SOURCE.md`。全部资源已在
`~/.novel-tts/voxcpm/models/2b-bf16/32279effe8c19989596f05d353d1447f51d9e915/`
校验，`source-manifest.json` 保存各文件 SHA-256。BF16/FP16 复用同一原始
Safetensors，不从 Q8 解量化来冒充原始精度；无需发布推理 Python。

`Model::load_original` 直接读取原始名称，统一 Candle 0.9.2；网络使用指定
BF16/F16，AudioVAE F32。官方 `.pth` 在 `state_dict` 下，Rust 在初始化时
合并 `weight_g/weight_v`。当前原始路径通过显式 benchmark/API 验证，尚未
注册为阅读器模型，既有 Q8 模型、音色缓存和默认选择继续保持。

CFM 每个模型只缓存一个步数对应的时间嵌入表，并把不变的 cond projection
移到每个 patch 的迭代循环之外。保留 CFG 双分支 batch、固定噪声、步数、
公式和取消检查。实际 F32 固定噪声 oracle、10→4→10 步切换、取消后复用
通过；Q8 五轮最后一轮的 11 段 WAV 与优化前逐字节一致。

### 同机串行 Release 五轮

仍用同一 11 段中文语料、参考音色/文字、seed42/10 步/CFG2/温度1/上限200，
每条路径先预热一次再测五轮；本任务未并行运行构建或其他推理。逐段 WAV
和 summary 保存在 `target/tts-integration/voxcpm-{cfm-cached,original-bf16,original-f16}/`，
汇总 `voxcpm-precision-comparison.json`；相应 `*-metrics/` 保存 RSS、整卡显存
采样和原始 stdout/stderr。

缓存前的独立 Candle 基准也保留为
`voxcpm-candle-baseline/candle-before-cfm.exe`，SHA-256
`053f2ae71b3be8c2708f07dca6af8a38a4240961e6b2bfaa8d37f76e45155bd6`。

| 路径 | 五轮 RTF | 平均 RTF | 热首 PCM ms | 冷加载 ms | 参考编码 ms | 峰值整卡显存 MiB |
| --- | --- | ---: | --- | ---: | ---: | ---: |
| GGUF Q8 + F16，CFM 缓存后 | .509582 / .511230 / .514833 / .533276 / .543255 | .522435 | 95 / 97 / 97 / 96 / 108 | 4476 | 219 | 6298 |
| 原始 BF16 + F32 VAE | .505054 / .508487 / .498176 / .496948 / .495025 | .500738 | 86 / 80 / 84 / 91 / 83 | 4616 | 108 | 7173 |
| 原始 FP16 + F32 VAE | .519925 / .506360 / .496218 / .502118 / .504148 | .505754 | 88 / 85 / 79 / 85 / 81 | 4301 | 105 | 7365 |

每轮音频长度分别为 54.72 / 55.04 / 54.88 秒，所有片段正常 EOS。
原始 BF16 平均 RTF 比缓存后的 Q8 低约 4.2%，没有数量级提升。
单独 CFM 缓存前后 .526085→.522435，变化约 0.7%，接近波动，不能宣称
已经显著优化。原始路径同时改变权重、网络/codec 精度和 tokenizer，不能
把结果归因于文件容器本身。原生 llama.cpp-omni 的既有热平均 .254589
仍明显更快；官方 PyTorch GPU 同机性能仍未测量。

峰值进程 RSS 分别为 935952384 / 5016657920 / 4653322240 字节；原始加载
使用 mmap，峰值包含权重文件驻留页。加载后的原始 BF16 热 RSS 约 743MiB，
不是持续占用 5GB。显存数值包含桌面等其他进程，不是本进程独占显存。
三条路径取消后再生成、空输入/非法参数/帧数截断/迭代取消、立即初始化取消
均通过。首 PCM 后取消返回耗时小于 1ms 的计时分辨率；物理释放为
26 / 43 / 43ms。原始模型的加载进行中取消、无文字克隆和设计完整矩阵仍
未单独实测。原始 CPU 只完成 F32 模块 oracle，不宣称 CPU 整机播放验收。

### 原始权重数值和前端

原始官方 F32 对照由 `voxcpm_reference.py --original-models` 生成，再由
`VOXCPM_ORACLE_ORIGINAL=1` 运行 Rust。结果：BaseLM 241/6144、完整与增量
BaseLM 155/6144、LocalEncoder Transformer 1/3072 超容差，共 **397**；
最大绝对差 **0.0003528595**。该记录与既有 GGUF 解量化的 31 项是不同的
权重对照，不能混为同一结果。严格阈值不变，不假定误差已经证明仅为舍入。

ResidualLM、完整 LocalDiT、编码投影、FSQ、固定噪声 CFM、AudioVAE 原始
编码/解码/分块/重置均通过相同 F32 阈值。CFM 最大绝对差 .0000033155084，
VAE 编码 .000008821487，解码 .00000009671203。日志
`voxcpm-original-rust-f32.log`，因此完整数值验收仍未通过。

官方实际 LlamaTokenizerFast + 固定源码中文拆分与 GGUF/SentencePiece 的
17 条夹具有 3 条不同，包括首部空白和 BPE 合并。分别保留
`tokenizer.json` / `original-tokenizer.json`，Rust 原始前端全部匹配官方
17 条。差异材料 `voxcpm-tokenizer-differences.json`；不由该差异直接推断
听感或漏字原因。

三条路径 round-5 的 `transcription.json` 均已生成（Whisper small/int8 CPU）。
数字/日期和英文混读能够被识别；简繁、他/她、同音词以及“山风/山峰”、
“石阶/时间”等差异仍需逐段试听。自动转写不是音质或内容正确性的最终判定。
原始 BF16/FP16 没有人工听感、30 分钟真实播放或 Linux CUDA/macOS Metal
验收，不将其列为已完成产品资格。

### 本次构建与产品回归

全工作区 all-features 测试通过：536 项通过、7 项真实模型测试跳过，
另行运行的真实模型测试结果如上。Clippy（所有 targets/features，警告即错误）、
fmt、rustdoc（examples 及库私有项，警告即错误）均通过。CPU-only Vox worker
和无后端裁剪构建检查通过；阅读器 normal 依赖树未包含 Candle、Vox、ORT
或 novel-tts-backends。首次多任务构建触发 Windows 页面文件不足，改用
`-j1` 后完整重跑通过，没有修改系统页面文件。

新的 all-features release worker 已构建，SHA-256：
`53f3ee9f93f761bf0aa6605b25fcb86e3db984965560387ff5ca89d3bb1bb0e5`。
通过协议 v5 对生产 Q8 路径完成 15 秒实际静音播放、停止、取消后下一请求
正常完成；未发生缓冲欠载。日志和隔离配置位于
`target/tts-integration/voxcpm-precision-product-smoke/`。
这是短时产品回归，不替代 30 分钟验收或人工听感。

## 远程同步与原始权重阅读器实验（2026-10-07）

提交前同步远程 `tts-gpu-model-integration` 的 `1ab87ee` / `8d13984`：
移除 Kokoro/ZipVoice，保留 Metal 修复，升级 ORT rc.13 / Runtime 1.28，
取消 Intel Mac 发布目标。本机增加 CUDA 13.0 Update 2（nvcc 13.0.88）
与 cuDNN 9.14 CUDA13，NVIDIA 官方组件清单逐包校验 SHA-256；12.9 保留。
Windows 系统 CUDA_PATH/CUDA_HOME/PATH 已更新，CUDA 13 DLL 在 bin/x64。

用户随后明确请求阅读器实验入口，因此原始 BF16 从纯计算候选扩展为
CUDA 限定的 `voxcpm/2b-bf16` 实验项。它不是默认或已通过资格的模型；
设置显示实验名称和“数值与长期稳定性验收待完成”。原始权重清单固定四个
运行所需文件，复用已有默认缓存；音色、参考编码及校准按原始 revision
隔离。切换后旧模型释放，音色回到目标模型默认，可独立导入相同参考 WAV。
CLI 导入、设计和播放均按所选模型执行。上述严格数值失败、人工听感、
30 分钟连续播放和外平台验收继续保持未通过/未完成。

## Candle 0.11.0 / CUDA 13 升级复测

用户授权统一升级最新稳定 Candle 0.11.0（打包源码
`31f35b147389700ed2a178ee66a91c3cc25cc80d`），core/NN/transformers
精确同版本，没有引入私有 fork 或第二套 Candle。Vox 参考编码实现身份和
所有 Candle 后端的性能校准 runtime_info 同步版本，旧 WAV/权重继续复用。

Windows RTX 5070、CUDA 13.0.88、cuDNN 9.14.0.64、同一驱动和 11 段语料，
同一参考 WAV/文字和 seed42/10步/CFG2/温度1/上限200。每条路径先预热一次，
五轮串行，测试时没有构建或其他推理负载。原始采样和 WAV 在
`target/tts-integration/candle011-{q8,bf16}-{metrics,wavs}/`。

| 路径 | 五轮 RTF | 平均 RTF | 热首 PCM ms | 冷加载 ms | 参考编码 ms | 峰值整卡显存 MiB |
| --- | --- | ---: | --- | ---: | ---: | ---: |
| 0.11 Q8 + F16 | .521383 / .521104 / .512432 / .512374 / .521517 | .517762 | 85 / 107 / 85 / 87 / 80 | 4585 | 207 | 5738 |
| 0.11 原始 BF16 + F32 VAE | .525281 / .515529 / .512390 / .511793 / .506000 | .514199 | 100 / 94 / 81 / 82 / 83 | 5143 | 109 | 6782 |

各轮音频分别为 54.40 / 55.04 秒，全部正常 EOS。每轮整体吞吐满足 ≤0.8；
BF16 平均仅比 Q8 低约 0.7%，没有证据说明换原始格式会显著加速。
保留的 0.9.2 Q8 release 程序也在同机串行重测五轮：
.519574 / .531876 / .528818 / .525172 / .527139，均值 .526516，
热首 PCM 94 / 98 / 99 / 112 / 100ms；冷加载 4464ms，参考编码 229ms，
释放 29ms，取消及错误回归通过。程序 SHA-256
`e6b3b0bb85fa945bba3e00e1d6ec28ee514aecbad2830072ba72abbd1c161231`，
输出 `candle092-repeat-{metrics,wavs}/`。新版 Q8 RTF 低约 1.7%，接近运行
波动，不声明显著加速；新旧音频总时长 54.40 / 54.72 秒，也不以相同 seed
推断数值或听感相同。旧程序链接 CUDA12，新程序使用 CUDA13，不能将差异
全部归因于 Candle。
峰值 RSS 分别为 953376768 / 4315205632 字节，BF16 加载峰值包含 mmap
驻留页；整卡显存包括桌面，不能解读为模型独占显存。
取消首 PCM 后返回低于毫秒计时分辨率（记录 0ms），物理释放 25 / 50ms。
两条路径空输入、异常参数、帧数截断、预填充/扩散取消、初始化立即取消、
取消后下一请求均通过。

全工作区 all-features 537 passed / 7 ignored；Clippy all-targets/features
`-D warnings`、fmt、rustdoc（库私有项和 examples）通过。release 阅读器、
worker 和独立基准构建通过；CPU-only / 无后端裁剪检查、阅读器依赖隔离
通过。worker SHA-256：
`7e4613bd12f48ac5e75d3023083b62293f97639d50b4bfe55a9da6d3bbc29968`。
CUDA 13 官方 cuda.lib loader 的 LIBCMT 导致 LNK4098，保留诊断，不用
全局 NODEFAULTLIB 隐藏。Q8 CUDA 与 BF16 Auto 实际静音播放和切换成功。

ORT 1.28 CUDA13 在本机 Nano `/Cast` 生成失败：分发仅含 sm75/sm80/sm90a
且没有 PTX，缺少 sm120 内核。没有以 Candle 成功推断 ORT/CUDA 共存推理
通过；显式 CUDA 不降级，详细证据见 `dev-notes/ort-rc13-upgrade.md`。
此前 0.9.2 的严格真实权重数值失败（GGUF 31 / 原始 397）属于历史记录，
当前 0.11.0 未重新完成这组官方真实权重 oracle，不宣称已解决。
当前 WAV 的人工试听、自动转写及 30 分钟实际播放仍未完成；Linux CUDA、
macOS Metal 和最低 Rust 1.89 未在此 Windows 环境验收。

产品补测：`candle011-other-switch/progress.json` 保存 Qwen 0.6B CUDA、
OmniVoice CUDA、MOSS Nano CPU 三条实际播放通过记录；随后尝试 MOSS 1.7B
CPU 被设备门控正确拒绝（该产品档仅开放 GPU），不是生成失败。
`candle011-moss-switch/result.json` 的 MOSS Realtime 1.7B CUDA、Local 1.7B
CUDA及切回 Vox Q8 全部正常完成；每次切换先释放旧模型，准备前整卡显存
约回到 1.6GiB。Nano ORT CUDA 的单独失败记录仍保留。
BF16 15 秒实际静音播放有 3 段、零缓冲欠载，stop 回执低于毫秒计时分辨率，
随后下一请求正常完成（`candle011-bf16-playback/result.json`）。这是短时
产品回归，不代替 30 分钟稳定性或人工听感验收。

CPU Q8 无参考短句预热后生成正常 EOS：2.72 秒音频、生成 14998ms、RTF
5.514240、首 PCM 2224ms；冷加载 4640ms，释放 288ms，峰值 RSS
5084049408 字节。空输入/异常参数/截断/取消及取消后复用通过，材料在
`candle011-cpu-{metrics,wavs}/`。这是 CPU 功能 smoke，不是完整五轮基准，
也未达到实时吞吐。实际 release worker 驱动的渲染 TUI 测试
`experimental_voxcpm_model_is_labelled_and_can_switch_back` 通过：Q8→BF16
实验提示、CUDA 能力约束、切回 Q8 时提示消失；日志 `candle011-tui-experiment.log`。
