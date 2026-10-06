# novel-tts-protocol

轻量协议 DTO，无模型或音频依赖。协议模式的 stdin/stdout 为 UTF-8 JSON Lines；日志写 stderr，音频不经管道传输。

首条请求是 `hello`，仅接受 `protocol_version: 1`。`request_id` 在同一连接中唯一；重复 ID 不再执行。响应回显 ID，异步事件的 `request_id` 为 null。客户端先校验 `instance_id`，再校验 `session_id` 和递增 `sequence`，拒绝旧进程/旧章节事件。

```json
{"protocol_version":1,"request_id":"hello-1","session_id":null,"type":"hello"}
```

后续命令为 get_status/get_config/update_config、prepare_model/cancel_prepare、start/pause/resume/stop/seek/shutdown。带参数的命令使用 `payload` 对象，详情见公开 Rust DTO。命令接受（accepted）与播放完成（session_ended）是不同事件；完成原因分别为 completed/cancelled/failed。

单行编码后上限为 16 MiB（不含行尾）。读取方必须在解码前限制缓冲大小，不能先无限读取再校验。正文换行由 JSON 转义，解码后保留原始字节；TextRange 为原文 UTF-8 左闭右开字节范围。

`tests/fixtures` 是本项目自编的可分发语料与配置样例，不需要下载模型。试听和性能验收由听书程序执行，协议测试不证明音质或实时性。
