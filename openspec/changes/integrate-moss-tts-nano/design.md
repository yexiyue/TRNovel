# Design

## Context

当前独立听书程序通过 JSON Lines 与阅读器通信，核心运行在 LocalSet，会话检查点使用原文 UTF-8 字节坐标。Kokoro 模型固定依赖 ort rc.10，需继续支持现有发布目标。

## Goals / Non-Goals

**Goals:** 原生 CPU MOSS 推理、流式播放、CLI 参考音色管理、能力驱动的 TUI，以及模型无关的会话核心。

**Non-Goals:** Python 部署依赖、GPU、动态插件、情绪控制、多角色自动分析。

## Decisions

- 具体模型和分词依赖移入 novel-tts-backends；core 拥有音频设备和通用会话，CLI 组装后端。模块采用 foo.rs 与 foo/ 子模块。
- 后端输出有界 PCM 流，以显式 End 表示成功结束；断连和模型限制触发失败，不能视为文本已读完。Kokoro 单段输出适配同一流接口。
- MOSS SentencePiece 50-token / 60 个 CJK 字符预算合并相邻句子，超限优先在句末分段，原文范围独立于合成文本规范化；检查点仅在实际播放完成后提交。
- CPU ORT 会话在专属线程上构造、执行和销毁；只有拥有所有权的文本、token、PCM、命令跨线程。丢弃流触发步骤间取消。
- 固定 TTS revision f52645cb467506d8e18e746ddd59482685b74e58 与 codec revision ceff0d0749bfb3fa2d61149794ec6feef0d1e1ae，校验尺寸与 SHA-256，复用断点下载。损坏文件隔离后允许重试。
- 参考音频仅支持 1..30 秒非静音 mono/stereo WAV，内部重采样至 48kHz stereo。编码后的自定义音色记录稳定 ID、名称和模型 revision，原子保存并拒绝覆盖。
- 新配置默认 moss/Weiguo；旧 JSON 未声明 backend 时按 Kokoro 解读。不可用后端报错并允许用户显式切换。
- 切换后端停止旧会话并取消资源准备，释放模型和设备，保留续读检查点；用户手动准备并开始新会话。
- 协议 v2 的 Ready 携带所有已编译后端能力与音色名称；阅读器与 worker 配套升级。

## Risks / Trade-offs

- ONNX 图加载及数值一致性需真实模型验证，不能以编译成功代替。跨平台听音需要对应设备环境。
- 流式缓冲允许生成与播放重叠，但必须保留有界背压，并在取消后隔离所有旧会话输出。
- 模型总大小约 763 MB，首次准备成本较高；复用缓存，按需准备后端。
- 固定采样图包含官方采样参数，首版不开放参数调节。参考音色随模型版本变化需要重新导入。
