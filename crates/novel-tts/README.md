# novel-tts

MOSS GPU 试用可增加 `moss-candle-cuda`（Metal 构建入口为
`moss-candle-metal`），模型 ID 为 `local-1.7b`、`realtime-1.7b`。
已有 MOSS 配置继续 Nano；CLI 可用 `--model nano --tts-device cpu` 切回。
Local 速度仍未达主力要求，Realtime 更适合先试听，两者均标为实验。

```sh
cargo build --release -p novel-tts --features qwen-cuda,voxcpm-cuda,omnivoice-cuda,zipvoice,moss-candle-cuda
novel-tts --backend moss --model realtime-1.7b --tts-device cuda voices design myvoice --name "我的音色" --description "温暖清晰的成年女性普通话"
novel-tts --backend moss --model realtime-1.7b --voice custom:myvoice --tts-device cuda book.txt
```

音色设计使用 VoiceGenerator/BF16，保存参考后供 Local/Realtime 克隆，不逐段
重新设计。参考暂限 1–10 秒；导入时需要 `--text` 记录原文。所有权重放默认
`.novel-tts/moss/models/<id>/<revision>`，共用一份 codec。完整大模型 CPU
未验收，worker 试用模式只公开 GPU，CPU 听书仍使用 Nano。详情及未完成验收
见 `dev-notes/moss-candle-acceptance.md`。

独立朗读 UTF-8 文件，不需要启动阅读器，不再启动另一层听书子进程。

本次命名迁移由原 `novel-tts` 库拆出 `novel-tts-core`，CLI 接管 `novel-tts` 包名与命令。当前工作区保留 0.3.0 版本线，后续发布需升级版本；已发布的旧库不包含本 CLI，请先使用下面的源码构建命令。

```sh
cargo build -p novel-tts
cargo run -p novel-tts -- book.txt
cargo run -p novel-tts -- --restart book.txt
```

默认启用原生 CPU MOSS-TTS-Nano，首次实际启用会下载固定版本模型并校验，再打开音频设备。添加 `--features kokoro` 同时编入 Kokoro，或用 `--no-default-features --features kokoro` 单独构建它。单纯协议握手、查询设置或状态不下载、不加载、不播放。模型分别位于 `~/.novel-tts/moss/` 和 `~/.novel-tts/kokoro/`，`--model-dir` 指定包含这些后端子目录的公共根目录；`--config` 和 `--checkpoint-dir` 可用于隔离运行。

交互终端：空格暂停/继续，s 停播并退出，q/Esc 退出，Ctrl+C 退出；按键模式退出时恢复终端。非交互输入不启用原始模式，正文完成后退出，也支持 Ctrl+C。文件不可读/非 UTF-8 在准备模型前失败。成功/主动停止返回 0；文件、模型、设备或会话错误返回非零，参数错误由 clap 返回 2。状态输出写 stderr，不把正文/音频发到 stdout。

## JSON Lines

```sh
printf '%s\n' '{"protocol_version":5,"request_id":"1","session_id":null,"type":"hello"}' '{"protocol_version":5,"request_id":"2","session_id":null,"type":"get_config"}' '{"protocol_version":5,"request_id":"3","session_id":null,"type":"shutdown"}' | target/debug/novel-tts --protocol
```

完整调用顺序：hello → get_config → prepare_model → 等 model_ready → start。start payload 是 source、text、text_hash、resume_byte、restore_checkpoint，摘要必须为原文 UTF-8 SHA-256。控制命令带当前 session_id；seek payload 额外带 byte 和 new_session_id，返回新会话 ID。accepted 仅表示接受，只有当前正文的 session_ended/completed 表示已播完。

每条 stdout 行为协议 JSON，日志写 stderr；协议模式不读取终端键位，不进入原始模式。首次 hello 超时为 5 秒，行上限为 16 MiB；未知版本关闭连接，非法 JSON/重复请求返回错误，不执行播放。EOF/shutdown 停播并收尾，保留最近播放检查点。取消准备保留 .download 半文件，下次创建新下载任务可续传；服务器忽略 Range 时重头下载。下载文件锁阻止多个进程同时修改半文件。

实际终端按键、主观音质和各平台 30 分钟持续播放仍需人工验收；自动化协议测试不替代这些结果。独立程序可从 workspace 构建。默认听书发行包包含三份同目录二进制，基础版包只有两个阅读器；双变体发行尚未发布。详见安装指南及 `dev-notes/tts-acceptance.md` 中的真实检查记录。

## 后端与音色

新配置在可用 CUDA/Metal 且编入 Qwen 时默认选择 1.7B-CustomVoice，否则按 CPU MOSS → Kokoro → Qwen 0.6B 选择；已有明确配置保留，旧文件未声明 backend 时解释为 Kokoro。未编译后端会报错，必须显式切换。下面的后端/音色选项保存到听书配置：

```sh
novel-tts --backend moss --voice Weiguo book.txt
novel-tts --backend kokoro --voice Zf001 book.txt
novel-tts --backend qwen --voice uncle_fu book.txt
novel-tts --backend qwen voices list
novel-tts voices list
novel-tts voices import narrator --name "我的朗读音色" reference.wav
novel-tts --backend moss --voice custom:narrator book.txt
novel-tts voices remove narrator
```

音色导入接受 1..30 秒非静音 mono/stereo WAV，自动重采样与编码，并拒绝覆盖同名音色。删除仅限自定义音色。导入音色后重新打开阅读器听书连接以刷新目录。

### 模型与设备

| 后端 / 模型 ID | 资源大小（十进制） | 输出与用途 |
| --- | --- | --- |
| qwen / 0.6b-customvoice | 约 2.50 GB | 保留旧模型、九种预置音色 |
| qwen / 1.7b-customvoice | 4.52 GB | GPU 主力、预置音色、风格描述 |
| qwen / 1.7b-base | 4.54 GB | 保存参考音色后渐进生成 |
| voxcpm / 2b-q8_0 | 3.55 GB | Q8 BaseLM + F16 Acoustic，原生流式与克隆 |
| omnivoice / 0.6b | 3.27 GB | 语义段生成、克隆、标签设计音色 |
| zipvoice / distill-int8 | 184 MB | CPU 克隆，8 步离线语义段 |
| zipvoice / distill-fp32 | 549 MB | CPU 质量对比，独立音色与缓存 |

只下载启用的模型。原 Qwen 未声明 model 的配置仍解释为 0.6B。
新模型位于 `~/.novel-tts/<backend>/models/<model>/<revision>/`；旧 Qwen 0.6B、MOSS 和 Kokoro 目录保持不变。
阅读器与 worker 使用协议 5，必须一起更新。目录返回模型 ID、名称、能力与编译设备；运行时另外检测可用设备。
切换后端或模型先停止会话并释放旧模型，再显式准备新模型；已有音色在阅读器中选择。

```sh
# Windows / Linux GPU；保留 MOSS 和默认关闭的对齐
cargo build --release -p novel-tts --features qwen-cuda,voxcpm-cuda,omnivoice-cuda,zipvoice
# macOS GPU
cargo build --release -p novel-tts --features metal,voxcpm-metal,omnivoice-metal,zipvoice
# CPU 补充；也可以只启用 zipvoice
cargo build --release -p novel-tts --features zipvoice
novel-tts --backend qwen --model 1.7b-customvoice --tts-device cuda --style "温暖沉稳，适合小说旁白。" book.txt
novel-tts --backend voxcpm --model 2b-q8_0 --tts-device cuda book.txt
```

Qwen 1.7B-CustomVoice 支持 `--style`，传空字符串清除。0.6B 和 Base 不接收风格描述。
显式设备不可用会报错；Auto 使用实测校准。VoxCPM2、OmniVoice、ZipVoice 都是按需 feature，不改变已有用户选择。
NVIDIA 编译需要 CUDA Toolkit 和 C++ 编译器；Windows CUDA 与 ORT 共存需要动态 CRT，具体设置见工具链笔记。

### 克隆与可复用设计音色

```sh
novel-tts --backend qwen --model 1.7b-base voices import narrator --name "旁白" --text "参考音频的准确文字" reference.wav
novel-tts --backend qwen --model 1.7b-base --voice custom:narrator book.txt
novel-tts --backend qwen --model 1.7b-base --tts-device cuda voices design warm --name "温暖男声" --description "沉稳温暖的成年男性，普通话清晰，适合小说旁白。"
novel-tts --backend qwen --model 1.7b-base --voice custom:warm book.txt
novel-tts --backend voxcpm voices import narrator --name "旁白" --text "参考音频的准确文字" reference.wav
novel-tts --backend voxcpm --tts-device cuda voices design warm --name "温暖男声" --description "A deep, calm male voice"
novel-tts --backend omnivoice --tts-device cuda voices design narrator --name "旁白" --description "男，中年，低音调"
novel-tts --backend zipvoice --model distill-int8 voices import narrator --name "旁白" --text "参考音频的准确文字" reference.wav
novel-tts --backend zipvoice --model distill-int8 --voice custom:narrator book.txt
novel-tts --backend zipvoice --model distill-int8 voices remove narrator
```

Qwen VoiceDesign 仅生成短参考，保存到 1.7B-Base 音色库，长文使用 Base 复用提示；设计与 Base 资源分别按需下载。
VoxCPM2 和 OmniVoice 也只设计一次参考片段，后续朗读使用缓存的克隆提示。
Omni 描述要求模型支持的性别、年龄、音调等标签，非法标签会明确报错。
克隆要求参考 WAV 和准确文字：Qwen 接受 1..15 秒，其余新后端接受 1..30 秒。
音色按模型和 revision 隔离，不共享专用编码缓存，Zip FP32 对比需要再次导入参考。

固定来源、SHA-256 和验收记录见 `dev-notes/tts-model-tiers-acceptance.md`。
Omni 音频 tokenizer 权重另有 BOSON/Higgs/Llama 许可；Zip 英文前端的独立 eSpeak 工具为 GPL-3.0-or-later。
这些组件不能笼统称为 Apache/MIT，发行需要附带对应许可和源码材料。

## 执行设备与逐句高亮

默认编入 MOSS 与 Qwen CPU 对齐。可用以下构建与配置：

```sh
cargo build --release -p novel-tts --features coreml # Apple Silicon
cargo build --release -p novel-tts --no-default-features --features qwen-cuda # Qwen NVIDIA
cargo build --release -p novel-tts --features ort-cuda   # NVIDIA Linux/Windows
novel-tts --tts-device auto --alignment-device auto book.txt
novel-tts --tts-device cpu --alignment-device cpu book.txt
```

auto 对两个组件独立预热 3 次、测量 5 次，完整链路至少快 15%、首音频不慢超过 10% 才选加速；同时使用加速时再校准并发竞争。开发构建测量不代表发布构建性能。缓存按硬件、模型 revision、ORT 版本保存于模型根目录的 calibration-*.json；删除可重新校准。显式不可用设备返回错误。Kokoro 当前仅支持 CPU 合成，也可使用 Qwen 对齐。

Qwen CPU Q4 约 0.99 GiB，GPU 浮点权重约 3.42 GiB，独立按需下载并校验。资源在 `~/.novel-tts/alignment/qwen/`；各模型目录下的 coreml-cache 可删除重建。CoreML 使用 MLProgram 和静态子图，允许系统选择 GPU/神经引擎，但仍有 CPU 算子。CUDA 的 KV/codec 状态通过 I/O binding 保留在设备端，实机性能仍须验收。

立即流式播放并显示片段高亮；异步对齐成功后切为逐句高亮。对齐资源缺失、推理失败、积压或超时仍继续播放。自动设备运行失败后后续任务重建 CPU 路径，已输出音频不重播；失败合成块不提交完成检查点。详见 `dev-notes/continuous-tts-acceptance.md`。

## 可选逐句高亮

默认 `alignment_enabled=false`，准备和播放仅加载所选 TTS 后端。`novel-tts --alignment book.txt` 显式开启 Qwen，`novel-tts --alignment=false book.txt` 关闭；偏好写入听书配置。已有配置没有该字段时也关闭。对齐设备仅在启用时参与准备，切换开关停止当前会话并保留可靠续读位置。

MOSS 合并排版单换行，空行/标题/分隔线保留边界。使用 8 秒目标 / 12 秒预计上限及模型安全预算。模型超限或推理失败时停止，不跳过文本，也不自动重复已播放内容。正常 EOS 不保证逐字覆盖，详见 `dev-notes/moss-continuity-acceptance.md`。
