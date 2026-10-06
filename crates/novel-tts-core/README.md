# novel-tts-core

会话核心，供 `novel-tts` 的 CLI 和 JSON Lines 入口共同使用。阅读器只依赖 `novel-tts-protocol`，不链接本库。

## API 迁移

原 `novel-tts` 库更名为 `novel-tts-core`。消费者在 Cargo.toml 中使用 `tts-core = { package = "novel-tts-core", version = "0.3.0" }`，Rust 引用为 `tts_core`；本次改名尚未发布，当前工作区可使用对应 path 依赖。

此次重构移除 `NovelTTS`、`ChapterTTS`、`Player` 和无界音频 queue API，不再公开 Kokoro 或 rodio 类型。使用 `backend::Backend`、`player::Playback` 和 `session::SessionManager`；位置为 `TextRange` 原文 UTF-8 字节范围，结束原因是 completed/cancelled/failed。示例见 `examples/basic.rs`。

默认 `kokoro` feature 保持 kokoro-tts 0.3.1、ort 2.0.0-rc.10、rodio 0.21.1。关闭默认 feature 可使用无原生音频/推理依赖的核心接口。Kokoro 只支持片段生成，不支持原生 token 音频流、角色风格、克隆或发音提示。

`SessionManager` 和播放器运行于 Tokio LocalSet。设备只归播放线程，Kokoro 由专用推理线程构建、使用和销毁；线程之间只传拥有所有权的正文与 PCM。暂停停止消费；预取包与当前播放包共用 30 秒和 16 MiB 预算，消费后释放。模型推理自身的临时工作内存不属于音频队列预算。单段超过预算会失败，不静默截断。

速度 0.5..2 和音量 0..10 由播放层处理，音色 ID 保留旧 JSON 拼写，例如 Zf001。任何合成错误都停止会话。start、seek、切音色替换会话，旧会话取消，不跳过失败片段。合成结束不等于播放结束，片段进度来自播放边界，首版不承诺逐字对齐。

## 配置与恢复

唯一配置写入方是听书程序。配置保留 `~/.novel/tts_config.json` 和旧 volume/speed/voice/auto_play 字段，缺 backend 解释为 kokoro。读取、创建默认值和 Drop 都不保存；更新校验后用临时文件原子替换，保留未知字段，短文件锁及 revision 防止并发覆盖。损坏文件、未知 backend、非法设置报错，不重置文件。revision_conflict 后重新查询设置再主动修改。

模型仍在 `~/.novel-tts/kokoro/`。检查点在 `~/.novel/tts/checkpoints/`，文件名是来源 ID 摘要；内容带 schema_version=1、来源、原文 SHA-256、resume_byte、completed 和更新时间。CLI 使用 cli-file 命名空间，阅读器使用 reader，彼此不覆盖。正文变化、非 UTF-8 边界或未知版本报错；损坏数据也报错。用户可通过 CLI `--restart` 或面板「从本章开头重新播放」显式忽略旧恢复点。

每段开始前保存起点，结束后保存终点；整章耗尽后才保存完成。取消会等待正在提交的检查点事务，避免旧会话覆盖新位置。故障后最多重复最近未完成片段；不承诺采样级恢复。读取检查点不会自动播放。回退时保留配置、模型及检查点：旧程序忽略新增字段/检查点，阅读进度格式未改变。
