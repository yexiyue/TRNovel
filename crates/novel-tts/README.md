# novel-tts

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
printf '%s\n' '{"protocol_version":3,"request_id":"1","session_id":null,"type":"hello"}' '{"protocol_version":3,"request_id":"2","session_id":null,"type":"get_config"}' '{"protocol_version":3,"request_id":"3","session_id":null,"type":"shutdown"}' | target/debug/novel-tts --protocol
```

完整调用顺序：hello → get_config → prepare_model → 等 model_ready → start。start payload 是 source、text、text_hash、resume_byte、restore_checkpoint，摘要必须为原文 UTF-8 SHA-256。控制命令带当前 session_id；seek payload 额外带 byte 和 new_session_id，返回新会话 ID。accepted 仅表示接受，只有当前正文的 session_ended/completed 表示已播完。

每条 stdout 行为协议 JSON，日志写 stderr；协议模式不读取终端键位，不进入原始模式。首次 hello 超时为 5 秒，行上限为 16 MiB；未知版本关闭连接，非法 JSON/重复请求返回错误，不执行播放。EOF/shutdown 停播并收尾，保留最近播放检查点。取消准备保留 .download 半文件，下次创建新下载任务可续传；服务器忽略 Range 时重头下载。下载文件锁阻止多个进程同时修改半文件。

实际终端按键、主观音质和各平台 30 分钟持续播放仍需人工验收；自动化协议测试不替代这些结果。独立程序可从 workspace 构建。默认听书发行包包含三份同目录二进制，基础版包只有两个阅读器；双变体发行尚未发布。详见安装指南及 `dev-notes/tts-acceptance.md` 中的真实检查记录。

## 后端与音色

新配置默认 moss/Weiguo；已有明确配置保留，旧文件未声明 backend 时解释为 Kokoro。未编译后端会报错，必须显式切换。下面的后端/音色选项保存到听书配置：

```sh
novel-tts --backend moss --voice Weiguo book.txt
novel-tts --backend kokoro --voice Zf001 book.txt
novel-tts voices list
novel-tts voices import narrator --name "我的朗读音色" reference.wav
novel-tts --backend moss --voice custom:narrator book.txt
novel-tts voices remove narrator
```

音色导入接受 1..30 秒非静音 mono/stereo WAV，自动重采样与编码，并拒绝覆盖同名音色。删除仅限自定义音色。导入音色后重新打开阅读器听书连接以刷新目录。

Ready 的 payload 是后端能力数组，含 backend、default_voice、voices、voice_names 和功能标志。UpdateConfig 支持 backend 字段；切换时同时提交该后端的 voice。后端切换停止当前播放并释放模型，保留续读检查点，需要手动准备和重新播放。

## 执行设备与逐句高亮

默认编入 MOSS 与 Qwen CPU 对齐。可用以下构建与配置：

```sh
cargo build --release -p novel-tts --features coreml # Apple Silicon
cargo build --release -p novel-tts --features cuda   # NVIDIA Linux/Windows
novel-tts --tts-device auto --alignment-device auto book.txt
novel-tts --tts-device cpu --alignment-device cpu book.txt
```

auto 对两个组件独立预热 3 次、测量 5 次，完整链路至少快 15%、首音频不慢超过 10% 才选加速；同时使用加速时再校准并发竞争。开发构建测量不代表发布构建性能。缓存按硬件、模型 revision、ORT 版本保存于模型根目录的 calibration-*.json；删除可重新校准。显式不可用设备返回错误。Kokoro 当前仅支持 CPU 合成，也可使用 Qwen 对齐。

Qwen CPU Q4 约 0.99 GiB，GPU 浮点权重约 3.42 GiB，独立按需下载并校验。资源在 `~/.novel-tts/alignment/qwen/`；各模型目录下的 coreml-cache 可删除重建。CoreML 使用 MLProgram 和静态子图，允许系统选择 GPU/神经引擎，但仍有 CPU 算子。CUDA 的 KV/codec 状态通过 I/O binding 保留在设备端，实机性能仍须验收。

立即流式播放并显示片段高亮；异步对齐成功后切为逐句高亮。对齐资源缺失、推理失败、积压或超时仍继续播放。自动设备运行失败后后续任务重建 CPU 路径，已输出音频不重播；失败合成块不提交完成检查点。详见 `dev-notes/continuous-tts-acceptance.md`。

## 可选逐句高亮

默认 `alignment_enabled=false`，准备和播放仅加载所选 TTS 后端。`novel-tts --alignment book.txt` 显式开启 Qwen，`novel-tts --alignment=false book.txt` 关闭；偏好写入听书配置。已有配置没有该字段时也关闭。对齐设备仅在启用时参与准备，切换开关停止当前会话并保留可靠续读位置。

MOSS 合并排版单换行，空行/标题/分隔线保留边界。使用 8 秒目标 / 12 秒预计上限及模型安全预算。模型超限或推理失败时停止，不跳过文本，也不自动重复已播放内容。正常 EOS 不保证逐字覆盖，详见 `dev-notes/moss-continuity-acceptance.md`。
