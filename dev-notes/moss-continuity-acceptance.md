# MOSS 连贯性、完整性与可选对齐

日期：2026-10-06，Apple M4 Pro / macOS ARM64。对应 `moss-continuity-reliability`，代码未提交、未发布。

## 变更与边界

- protocol v3 增加 `alignment_enabled` / patch 同名字段，默认 false，旧配置缺少字段也关闭。CLI `--alignment` 开启、`--alignment=false` 关闭并保存；TUI “逐句高亮”开启后才展示对齐设备。
- 关闭时不下载、校验、加载、校准 Qwen，也不创建对齐任务。切换设置停止并卸载当前模型，保留检查点，用户重新启用模型和播放；磁盘资源不删除。
- MOSS 单换行作为软边界；空行、装饰分隔线、已有内置 TOC 规则匹配的短标题为硬边界。标题独立朗读。原文和 UTF-8 范围不变；只规范化合成副本。Kokoro 分段保持原样。
- 内置标题模式在轻量 protocol 的 headings 常量中共享，阅读器 TOC 与 core 边界识别使用同一组规则。基础阅读版依然不依赖 core、backends、ORT 或音频播放。
- 初始目标 8 秒 / 预计上限 12 秒、50 个规范化 token / 60 CJK 字符 / 375 帧；时长估算继续随音色更新。预计上限不是模型实际时长保证。
- 软换行不触发段落尾静音处理；句末引号跟随前句，正常的英文词间空格保留。流式播放、共享预算和首尾静音阈值沿用上次实现。

```mermaid
flowchart LR
    Source[原文快照] --> Blocks[软换行上下文与预算切分]
    Blocks --> Moss[MOSS 推理线程]
    Moss --> PCM[PCM 与明确 End 或失败]
    PCM --> Queue[有界连续播放队列]
    Queue --> Progress[实际播放完成检查点]
    Moss --> Evidence[可选模型诊断报告]
    PCM -->|显式启用时| Alignment[独立 Qwen 对齐]
```

## 诊断与已复现问题

`moss_probe` 使用 Weiguo / seed=42，记录 source UTF-8 范围、规范化输入、token 数、生成帧数、音频时长、首个处理后 PCM 延迟、结束类型、错误和分块 WAV。报告存放在指定输出目录，不默认记录用户正文。

结束类型为 `eos / frame_limit / cancelled / inference_failure`，留在 backends，不让 core 判断模型 token。帧数超限不再触发 GPU provider 重建。core 只在明确 End 且实际播放完成后提交块完成；流断开或错误停止，不进入下一块、不自动重读。

本地《蛊真人》第一章若干长对话共 562 UTF-8 bytes：

| 路径 | 输入 token | 生成帧 | 结束 |
| --- | ---: | ---: | --- |
| 不分段直接输入 | 150 | 375 / 30.00 s | frame_limit，报告失败 |
| 最终切分，第 1 块 | 29 | 125 / 10.00 s | EOS |
| 第 2 块 | 21 | 57 / 4.56 s | EOS |
| 第 3 块 | 24 | 91 / 7.28 s | EOS |
| 第 4 块 | 20 | 67 / 5.36 s | EOS |
| 第 5 块 | 33 | 124 / 9.92 s | EOS |
| 第 6 块 | 27 | 127 / 10.16 s | EOS |

所有分段源范围覆盖可朗读正文；范围之间只跳过布局空白。最初按句末达到 8 秒才切分的策略产生过实际 15.04 秒的块；最终改为下一句越过目标时优先已有完整句末，保留 12 秒作为单个长句/分句的预计上限。时长估算不能代替生成上限。

英文标题、跨行正文与空行输入最终生成 4 块，全部 EOS；中英文混合正文及标题也 EOS。另一段含 `a=b、x=-3` 的短正文保留运算符后，以 15 token 达到 375 帧上限。该例证明短输入仍可能不收敛；没有删除符号或把失败算成成功，也没有用时长猜测漏读。

两句上下文在最终固定 seed 下输出 60 帧 / 4.8 秒，较逐行时明显短。对照官方 ORT 1.22 实现也是 60 帧，230,400×2 PCM 与 Rust 完全相等（最大误差 0）。该例不是 Rust 音频链路截断，但不能把偏短解释为完整朗读。

**正常 EOS 下的内容完整性尚无独立人工核对。** 当前无法确认用户最初报告的每个漏读案例都来自帧数上限，也不能凭 EOS 或对齐成功宣称无漏字。

## 同文音频与准备成本

三行短正文、同一 Weiguo / seed=42：逐行生成 3 块，原始音频 14.08 s；最初合并 1 块为 18.64 s，其中内部静音最大 3.84 秒。收紧目标边界后的最终策略为 2 块，共 8.88 s。两者均经相同边界静音处理，提供 MP3。单个上下文的时长和韵律变化明显，块数下降不能直接证明听感改善；主观衔接和逐字覆盖保留人工验收。

- `target/moss-reliability/comparison-lines.mp3`
- `target/moss-reliability/comparison-context-final.mp3`；comparison-context.mp3 保留最初版本。
- 同文：`soft-lines.txt`；逐块 PCM、时间与输入报告分别在 `line-baseline/`、`line-comparison-final/`。

同一个 debug worker 构建、已下载且本地已校验资源、显式 CPU，`/usr/bin/time -l` 直接包裹 worker。单次准备测量（包含重新完整校验，不含下载）：

| 偏好 | 模型就绪 | 峰值 RSS | 峰值 footprint |
| --- | ---: | ---: | ---: |
| 对齐关闭 | 3.00 s | 944,160,768 bytes | 980,174,480 bytes |
| 对齐开启 | 6.00 s | 4,058,660,864 bytes | 4,035,841,744 bytes |

这是单次顺序测量，受缓存影响，不是统计性能基准或长时间播放峰值。关闭状态的准备事件不含 Qwen 资源；真实资源测试使用只有 MOSS 符号链接的新目录，验证没有创建 alignment 目录、没有装配 aligner。默认 worker 三行输入以两个块连续播放，范围 0..97 / 98..134，分别实际播放完成后发送 SegmentFinished，最终 SessionEnded=completed，没有 SentenceStarted。

## 复现

```sh
cargo run --release -p novel-tts-backends --example moss_probe -- <moss-dir> input.txt output-directory
# 对照独立逐行合成；只适合已选取的短行，不用于生产切块。
cargo run --release -p novel-tts-backends --example moss_probe -- <moss-dir> input.txt line-output lines
# 长输入边界探针，故意绕开切分；失败将非零退出并留下报告。
cargo run --release -p novel-tts-backends --example moss_probe -- <moss-dir> long.txt direct-output unsegmented
TRNOVEL_MOSS_MODEL_DIR=<moss-dir> cargo test -p novel-tts-backends -p novel-tts
```

固定模型 revision、ORT rc.10、CPU/CoreML/CUDA 选择策略沿用 `continuous-tts-acceptance.md`。新的 OpenSpec 不归档前一变更的 NVIDIA 实机与人工对齐精度验收缺口。

## 验收状态

全 workspace 271 项测试、所有 target / feature 的 Clippy、rustdoc（warnings deny）、rustfmt、git diff --check、26 种 feature 组合和文档站 19 页面构建通过。真实模型测试另验证官方 fixture、EOS/取消/推理失败和关闭对齐不准备任何资源。

VHS 已查看默认关闭、跨行片段高亮、开启后出现对齐设备、再次关闭隐藏设备；失败提示另用明确的协议故障 fixture 验证，不能当作真实 MOSS 推理验收。代码和屏幕证据保存在 target/moss-reliability，portable tape 在 docs/tapes/optional-alignment.tape。

人工试听与逐字覆盖未完成，保留 OpenSpec task 4；不把自动测试写成人工验收。
