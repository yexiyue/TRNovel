# novel-tts-protocol

轻量协议 DTO，无模型或音频依赖。协议模式的 stdin/stdout 为 UTF-8 JSON Lines；日志写 stderr，音频不经管道传输。

首条请求是 `hello`，仅接受 `protocol_version: 3`。`request_id` 在同一连接中唯一；重复 ID 不再执行。响应回显 ID，异步事件的 `request_id` 为 null。客户端先校验 `instance_id`，再校验 `session_id` 和递增 `sequence`，拒绝旧进程/旧章节事件。

```json
{"protocol_version":3,"request_id":"hello-1","session_id":null,"type":"hello"}
```

后续命令为 get_status/get_config/update_config、prepare_model/cancel_prepare、start/pause/resume/stop/seek/shutdown。带参数的命令使用 `payload` 对象，详情见公开 Rust DTO。命令接受（accepted）与播放完成（session_ended）是不同事件；完成原因分别为 completed/cancelled/failed。

单行编码后上限为 16 MiB（不含行尾）。读取方必须在解码前限制缓冲大小，不能先无限读取再校验。正文换行由 JSON 转义，解码后保留原始字节；TextRange 为原文 UTF-8 左闭右开字节范围。

`tests/fixtures` 是本项目自编的可分发语料与配置样例，不需要下载模型。试听和性能验收由听书程序执行，协议测试不证明音质或实时性。

协议 v3 的 Ready payload 是已编译后端能力数组，每项含 backend、default_voice、voices、voice_names 及 streaming/cloning 等标志。UpdateConfig 可提交 backend 与 voice，阅读器不识别模型专属类型。v2 与 v3 不兼容，配套更新两个程序。

## v3 时间线与设备

`device_status` 分别报告 tts/alignment 的 compiled、available、selected 和原因；`auto` 仅在完成校准后确定设备。配置新增 `tts_device`、`alignment_device`，可为 auto/cpu/coreml/cuda。

`segment_started/finished` 对应合成块，`sentence_started/finished` 对应对齐句子，范围均指向原文。`alignment_status` 的 sentence_highlight=false 明确表示片段高亮，reason 说明缺资源、失败或超时；不得从合成顺序推测句子时间。句子事件有当前 session_id，迟到结果不能倒退高亮。播放速度不改变时间线的原始 PCM 帧坐标。

配置与 UpdateConfig 增加 `alignment_enabled`（bool / Option<bool>），默认 false，缺少字段的历史配置也关闭。该偏好独立于 alignment_device；关闭时不准备对齐资源。切换开关使当前会话停止，进度保留，用户重新开始。仍为协议 v3，两个程序配套升级。
