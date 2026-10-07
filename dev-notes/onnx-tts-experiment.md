# Qwen / OmniVoice ONNX 实验（2026-10-07）

本次新增 `qwen-onnx`（0.6B / 1.7B CustomVoice）和 `omnivoice-onnx`（0.6B）。它们是独立 Cargo feature、独立后端 ID，模型目录显示“ONNX（实验）”；没有新增协议字段，不参与默认后端选择，已有配置和 Nano Candle 改动保留。运行期只使用 Rust + 当前固定 `ort 2.0.0-rc.10`，Python 仅用于导出和对照。

## 来源与可复现导出

| 项目 | 固定版本 |
| --- | --- |
| [Qwen 导出器](https://github.com/zhangluoyang/qwen3-tts-onnxruntime/tree/5d1687f02855ee403792ed145fabc936f62b60f8) | `5d1687f02855ee403792ed145fabc936f62b60f8` |
| Qwen 0.6B CustomVoice 官方权重 | `85e237c12c027371202489a0ec509ded67b5e4b5` |
| Qwen 1.7B CustomVoice 官方权重 | `0c0e3051f131929182e2c023b9537f8b1c68adfe` |
| [OmniVoice 导出器](https://github.com/AFun9/Omnivoice-onnx/tree/4ec8125833a6a2806ff2e07b31e73f106954aad0) | `4ec8125833a6a2806ff2e07b31e73f106954aad0` |
| [OmniVoice Python](https://github.com/k2-fsa/OmniVoice/tree/08be0b4ccbac3e13e374e86fbfead4b4cac343e2) | `08be0b4ccbac3e13e374e86fbfead4b4cac343e2` |
| OmniVoice 官方权重 | `c5fdb5ccb189668d56333f77ba2629f4cd7535f4` |

全部是 FP32、CPU、Torch 2.5.1 legacy ONNX 导出，参考 ORT 1.22.0；Qwen 环境使用 transformers 4.57.3，OmniVoice 使用 5.5.0。开发环境分别位于 `target/tts-integration/nano-reference-venv`、`target/tts-integration/omni-export-venv`。上游 checkout 固定后，运行以下包装器；源码版本不匹配会报错。

```sh
target/tts-integration/nano-reference-venv/bin/python tools/tts/export_qwen_onnx.py \
  --upstream target/tts-integration/qwen-onnx-upstream \
  --source "$HOME/.novel-tts/qwen" \
  --output "$HOME/.novel-tts/qwen-onnx/models/0.6b-customvoice/85e237c12c027371202489a0ec509ded67b5e4b5" \
  --source-revision 85e237c12c027371202489a0ec509ded67b5e4b5

target/tts-integration/nano-reference-venv/bin/python tools/tts/export_qwen_onnx.py \
  --upstream target/tts-integration/qwen-onnx-upstream \
  --source "$HOME/.novel-tts/qwen/models/1.7b-customvoice/0c0e3051f131929182e2c023b9537f8b1c68adfe" \
  --output "$HOME/.novel-tts/qwen-onnx/models/1.7b-customvoice/0c0e3051f131929182e2c023b9537f8b1c68adfe" \
  --source-revision 0c0e3051f131929182e2c023b9537f8b1c68adfe

target/tts-integration/omni-export-venv/bin/python tools/tts/export_omnivoice_onnx.py \
  --upstream target/tts-integration/omni-onnx-upstream \
  --python-source target/tts-integration/omni-python-upstream \
  --source "$HOME/.novel-tts/omnivoice/models/0.6b/c5fdb5ccb189668d56333f77ba2629f4cd7535f4" \
  --output "$HOME/.novel-tts/omnivoice-onnx/models/0.6b/c5fdb5ccb189668d56333f77ba2629f4cd7535f4" \
  --source-revision c5fdb5ccb189668d56333f77ba2629f4cd7535f4
```

Qwen 包装器修正 0.6B attention 的 reshape：attention 投影输入宽度是 `16×128=2048`，不等于 hidden size 1024。大 FP32 text projection 在 Torch 内部形状推导达到 protobuf 2 GiB 上限时，仅跳过该次内部推导；完成后的图由 ORT 加载并做数值对照。OmniVoice 不开启图融合优化，避免额外非标准算子依赖。

每个模型的 `manifest.json` 记录 ABI、权重版本、导出源码版本、脚本 SHA-256、精度、参数、每张图的 opset（Qwen 18、OmniVoice 17），以及全部静态资源的尺寸和 SHA-256。复用现有下载校验入口验证本地导出；实验权重尚无发布下载 URL，在没有导出文件的机器上明确报缺失，不能直接在线自动安装。CoreML 编译缓存、克隆编码缓存、下载锁和临时下载文件不加入静态资源清单。没有删除已有模型缓存。含独立贪心对照图的静态资源分别为 Qwen 0.6B 4.665 GiB、1.7B 8.697 GiB，OmniVoice 2.984 GiB。

## 推理与比较条件

Qwen 复现 CustomVoice 提示构造、零长度 KV cache 预填充、逐帧缓存解码、显式 residual 随机数和 25 帧左上下文音频 codec。9 个固定音色及默认福叔保留，1.7B 支持最长 200 字情感描述。只支持 CustomVoice。

主采样参数与当前 Candle 一致：temperature 0.9、top-k 50、top-p 0.9、repetition penalty 1.05，seed 42，10 帧一块、最多 375 帧；遇 EOS 才完成，达到上限报错。Candle 的 residual 15 个码本是贪心，而上游 ONNX 默认采用 top-k 50 / temperature 0.9 随机采样。提示条件也有区别：Candle 预填充第一个正文 token，再逐帧加入剩余正文；ONNX 默认采用上游音频分块接口的完整正文预填充。为区分这些差异，两版模型另有 `--greedy-control` 导出的独立 `onnx-greedy` 图；开发测试通过 `NOVEL_TTS_QWEN_GREEDY_CONTROL=1` 同时使用贪心 residual 和 Candle 的逐步正文条件，不改变阅读器默认实验实现。不同实现的随机数算法不同，同一个 seed 不代表同一波形。

OmniVoice 复用当前 Rust 文本前端和 CFG 调度，32 步生成完整语义片段，8 个音频码本、mask token 1024；decoder 后按原实现去静音、归一化、fade/pad。默认音色及参考 WAV/text 克隆可用，参考素材与原后端共享，编码结果隔离在 `prompts/<export_revision>/<WAV SHA>.json`。没有宣称原生流式。

两种 ONNX 后端拥有独立推理线程，任务队列和音频队列容量均为 1；取消在图调用之间和有界发送时检查。正在执行的单次 ORT graph 无法立即中断，释放会等待当前调用结束。显式设备无法初始化时会报错；算子在成功注册的 CoreML 内回退到 CPU 会作为测试结果记录。

OmniVoice 参考 encoder 的 CoreML 分区在此固定 ORT 下产生错误形状，CPU `Slice_1` 报 `Ends must be a 1-D array`。实际克隆测试已确认，原始失败 profile 保留在 `profiles/fresh-omni/`。当前选 CoreML 时 encoder **明确固定 CPU**，LM 和 decoder 仍按请求注册 CoreML，没有自动重试或更换设备初始化；这不代表参考编码被 CoreML 加速。decoder 必须返回 `target_frames × 960` 个有效样本才可完成。

Candle GPU 默认 BF16，ONNX FP32；实际 Metal 性能对比必须保留这个标注。额外的 Qwen CPU FP32 + greedy control 试听用于对齐精度和采样模式。数值通过不等同于情感、漏字、重复、接缝等听感通过。

## 数值验证

- Qwen 导出器的 tokenizer encoder、decoder、text projection、talker prefill/cache decode、residual 验证通过。0.6B prefill 的 logits 最大绝对误差 `1.39e-4`、hidden `3.09e-4`；1.7B 缓存解码所有输出最大误差 `1.03e-4`。
- 实际 Rust Qwen 0.6B 提示嵌入和首轮 logits 与 Python ONNX 参考逐元素一致；同一组生成码本经参考的 10 帧分块 decoder 后，PCM 最大绝对误差 `1.07e-6`，50 帧和末尾样本数一致。
- Qwen 1.7B 开心描述的实际提示和首轮 logits 也逐元素一致，59 帧 PCM 最大误差 `9.48e-7`。0.6B 的贪心/逐步正文对照入口提示、trailing text、首轮 logits 一致，61 帧 PCM 最大误差 `7.51e-7`。
- OmniVoice 导出 encoder token 完全一致，decoder 最大绝对误差 `3.12e-6`。实际 Rust CFG 第一轮（batch=2）的 logits 对照 FP32 PyTorch 最大误差 `3.97e-4`；同一组最终码本的 decoder PCM 最大误差 `5.37e-6`，54 帧、51840 个原始样本一致。
- 对照脚本：`tools/tts/verify_qwen_onnx_trace.py`、`tools/tts/verify_omnivoice_onnx_trace.py`。开启 `NOVEL_TTS_ONNX_TRACE=<path>` 保存真实输入与码本，正常使用不保存。

## 手测入口

```sh
cargo build --release -p novel-tts \
  --features qwen-onnx,omnivoice-onnx,metal,omnivoice-metal,coreml,moss-nano-candle-metal,moss-candle-metal,voxcpm-metal
cargo run -- --tts-program "$PWD/target/release/novel-tts"
```

阅读器模型列表可选择“ONNX（实验）”项，再指定 CPU/CoreML。默认缓存已放到 `~/.novel-tts/qwen-onnx/models/` 和 `~/.novel-tts/omnivoice-onnx/models/`，这台 Mac 手测无需再次下载。独立 CLI 使用隔离配置，避免改动原配置：

```sh
target/release/novel-tts --backend qwen-onnx --model 1.7b-customvoice \
  --voice uncle_fu --style '非常开心，充满喜悦地讲述' --tts-device cpu \
  --config /tmp/trn-onnx-manual.json --restart /path/to/text.txt

target/release/novel-tts --backend omnivoice-onnx --model 0.6b \
  --voice custom:onnx-acceptance --tts-device cpu \
  --config /tmp/trn-omni-onnx-manual.json --restart /path/to/text.txt
```

`custom:onnx-acceptance` 是本次导入的新测试参考（福叔短句），没有覆盖旧音色；默认音色用 `narrator`。参考文本为“你好，欢迎使用听书功能。”。

## 实测与验收记录

测试机器：Apple M4 Pro、24 GiB、macOS 26.6.2；本轮不提供 CUDA 实测结论。所有正式性能测试逐项串行执行，不同时运行其他模型或构建。各条件预热一次，再连续生成三次取中位数；每个模型/设备的短句、段落、长文是三个独立进程，加载耗时取这三次加载的中位数。进程峰值覆盖加载和四次生成，不能解释为单次生成的额外内存。

### CoreML 分配与首次编译

以下是 ORT profile 实际执行的不同 kernel/分区节点数量，不是原图算子数量，也不能据此推断 GPU/ANE 使用率。完整汇总见 `dev-notes/fixtures/onnx-tts/coreml-allocation.json`。

| 模型/子图 | CPU kernel | CoreML kernel |
| --- | ---: | ---: |
| Qwen 0.6B text projection / codec embed / talker / codec decoder | 6 / 53 / 1279 / 620 | 0 / 0 / 0 / 0 |
| Qwen 0.6B residual | 801 | 451 |
| Qwen 1.7B text projection / codec embed / talker / codec decoder | 6 / 53 / 1279 / 620 | 0 / 0 / 0 / 0 |
| Qwen 1.7B residual | 816 | 466 |
| OmniVoice LM | 2738 | 28 |
| OmniVoice encoder（明确 CPU）/ decoder | 1306 / 643 | 0 / 0 |

冷启动使用单独的符号链接模型根目录，静态文件复用默认缓存，CoreML 编译目录独立；没有删除用户缓存。Qwen 0.6B 首次加载 100.08 秒，峰值 physical footprint 6.79 GiB；1.7B 首次加载 137.52 秒，峰值 14.74 GiB，过程中出现内存压力。正式 CoreML 基准复用这些编译缓存。OmniVoice 首次克隆的 LM CPU kernel 总时间 32.99 秒，CoreML kernel 总时间 0.19 秒，主要计算仍在 CPU。profile 仅用于分析，正式基准不启用 profiling。

### 功能与正确性验收

- 九音色、三种情感描述、OmniVoice 默认和参考音频克隆、取消后再生成，共 15/15 组功能用例通过；九音色分别覆盖 Qwen 0.6B 与 1.7B。
- 真实 worker 的八次后端切换通过，包括 Qwen 两版 ONNX、OmniVoice 默认/克隆、Candle Metal、Moss Nano CPU，再返回 ONNX；退出码 0，模型释放前后 RSS 有下降。只换同一个 OmniVoice 后端的音色不要求重载模型。
- Qwen 验证 EOS 和最后一块完整样本，OmniVoice 验证完整目标帧样本；无效音频 token、非有限值、Qwen 达到帧上限会报错。显式指定不支持的 ONNX Metal 设备立即报错。
- 原 Candle Qwen 的大词表 Metal 排序也存在超过 1024 项的问题，已采用 CPU 排序后返回原设备，保持推理模型仍在 Metal；1024/1025/2048/3072 项 CPU/Metal 回归通过。
- workspace tests、Clippy `-D warnings`、格式和 rustdoc `-D warnings` 全部通过，实验 release worker 和探针构建成功。

完整原始数据、WAV、CoreML profiles 放在 `target/tts-integration/`；可追踪的小型模型清单和 profile 汇总保存在 `dev-notes/fixtures/onnx-tts/`。数值与功能已验证；噪音、漏字、重复、情感和接缝的人工听审尚未验收，以下试听文件用于手测，不将数值通过公布为音质通过。

同精度情感/克隆和生成中途取消再试共 12/12 组通过。OmniVoice 默认和克隆各在提交后 100 ms 取消一次，再成功生成；保留接收器时销毁后端也能退出。原始结果快照见 `dev-notes/fixtures/onnx-tts/quality-results.json`，模型切换记录见同目录 `switch-results.json`。

### 成对试听

以下路径相对于项目根目录，均为本轮真实生成 WAV。前六个文件同时对齐 FP32、福叔、贪心 residual 和逐步正文条件；随机采样实现仍有差异，不要求逐点波形一致。

| 条件 | ONNX CPU | Candle CPU |
| --- | --- | --- |
| Qwen 1.7B 自然 | `target/tts-integration/quality/qwen-onnx-fp32-style-0.wav` | `target/tts-integration/quality/qwen-fp32-style-1.wav` |
| Qwen 1.7B 开心 | `target/tts-integration/quality/qwen-onnx-fp32-style-2.wav` | `target/tts-integration/quality/qwen-fp32-style-3.wav` |
| Qwen 1.7B 悲伤 | `target/tts-integration/quality/qwen-onnx-fp32-style-4.wav` | `target/tts-integration/quality/qwen-fp32-style-5.wav` |
| OmniVoice 默认 | `target/tts-integration/quality/omnivoice-onnx-fp32-narrator.wav` | `target/tts-integration/quality/omnivoice-fp32-narrator.wav` |
| OmniVoice 克隆 | `target/tts-integration/quality/omnivoice-onnx-fp32-custom-onnx-acceptance.wav` | `target/tts-integration/quality/omnivoice-fp32-custom-onnx-acceptance.wav` |

Qwen 九音色文件位于 `target/tts-integration/voices/0.6b-customvoice-nine-voices.<voice>.run0.wav` 和 `1.7b-customvoice-nine-voices.<voice>.run0.wav`。默认 ONNX 采样与原 Candle Metal 的情感试听在同一个 `voices/` 目录。

### 实验归档状态

用户选择统一维护 Candle 后停止剩余性能测试，正式基准完成 16/31 项，已完成项均成功；不能将其描述为完整基准。结果快照见 `dev-notes/fixtures/onnx-tts/benchmark-results.json`。Qwen/OmniVoice ONNX 留在实验分支，不合入正式 TTS 分支；模型和已有缓存保留。
