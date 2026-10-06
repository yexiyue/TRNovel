# novel-tts

独立朗读 UTF-8 文件，不需要启动阅读器，不再启动另一层听书子进程。

本次命名迁移由原 `novel-tts` 库拆出 `novel-tts-core`，CLI 接管 `novel-tts` 包名与命令。当前工作区保留 0.3.0 版本线，后续发布需升级版本；已发布的旧库不包含本 CLI，请先使用下面的源码构建命令。

```sh
cargo build -p novel-tts
cargo run -p novel-tts -- book.txt
cargo run -p novel-tts -- --restart book.txt
```

首次实际启用会下载缺少的固定 Kokoro v1.1 模型及音色，再打开音频设备。单纯协议握手、查询设置或状态不下载、不加载、不播放。模型保持 `~/.novel-tts/kokoro/`，可用 `--model-dir` 指定已准备目录；`--config` 和 `--checkpoint-dir` 可用于隔离运行。

交互终端：空格暂停/继续，s 停播并退出，q/Esc 退出，Ctrl+C 退出；按键模式退出时恢复终端。非交互输入不启用原始模式，正文完成后退出，也支持 Ctrl+C。文件不可读/非 UTF-8 在准备模型前失败。成功/主动停止返回 0；文件、模型、设备或会话错误返回非零，参数错误由 clap 返回 2。状态输出写 stderr，不把正文/音频发到 stdout。

## JSON Lines

```sh
printf '%s\n' '{"protocol_version":1,"request_id":"1","session_id":null,"type":"hello"}' '{"protocol_version":1,"request_id":"2","session_id":null,"type":"get_config"}' '{"protocol_version":1,"request_id":"3","session_id":null,"type":"shutdown"}' | target/debug/novel-tts --protocol
```

完整调用顺序：hello → get_config → prepare_model → 等 model_ready → start。start payload 是 source、text、text_hash、resume_byte、restore_checkpoint，摘要必须为原文 UTF-8 SHA-256。控制命令带当前 session_id；seek payload 额外带 byte 和 new_session_id，返回新会话 ID。accepted 仅表示接受，只有当前正文的 session_ended/completed 表示已播完。

每条 stdout 行为协议 JSON，日志写 stderr；协议模式不读取终端键位，不进入原始模式。首次 hello 超时为 5 秒，行上限为 16 MiB；未知版本关闭连接，非法 JSON/重复请求返回错误，不执行播放。EOF/shutdown 停播并收尾，保留最近播放检查点。取消准备保留 .download 半文件，下次创建新下载任务可续传；服务器忽略 Range 时重头下载。下载文件锁阻止多个进程同时修改半文件。

实际终端按键、主观音质和各平台 30 分钟持续播放仍需人工验收；自动化协议测试不替代这些结果。独立程序可从 workspace 构建。默认听书发行包包含三份同目录二进制，基础版包只有两个阅读器；双变体发行尚未发布。详见安装指南及 `dev-notes/tts-acceptance.md` 中的真实检查记录。
