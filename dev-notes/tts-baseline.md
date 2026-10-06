# Kokoro 重构基线

2026-10-06 在 Apple M4 Pro / arm64 / Rust 1.98.1 上，以现有听书库运行自编语料。使用 kokoro-tts 0.3.1、ort 2.0.0-rc.10、Zf001(1)，合成音频为 24 kHz 单声道 float PCM。旧 speed 和 volume 均在 rodio 播放层调整，基线录制时为 1.0，未改变合成模型速度。

资源摘要：

- kokoro-v1.1-zh.onnx SHA-256：`eefec708cbc7aba8e8129b5c2f7cb92e1fe7d281af1e1dd451592d9ff0714a0d`
- voices-v1.1-zh.bin SHA-256：`84ad1f8d2f4a1716365048360e77dbb2c8f55c0f076c577f28495bdab7cabfb0`

语料与逐段原文范围、G2P、token、样本数及本轮合成耗时保存在 `crates/novel-tts-protocol/tests/fixtures/`。WAV 和资源存放于 git 忽略的 `target/tts-baseline/`，不随源码或发行包提交。

复现：准备上述固定资源到目标目录，运行 `cargo run --locked -p novel-tts-core --example baseline -- target/tts-baseline`。文件不存在时命令报错，不自动下载模型。录制已完成；主观试听、变更前后盲听和各平台持续播放验收尚未完成，录制成功不代表音质通过。耗时包含本次启动条件，不能用作跨硬件或发布构建的性能结论。

原实现使用 200 UTF-8 字节分段，G2P 及模型保持不变。CRLF 的旧坐标累计只加 1 字节，迁移时修正为原始换行长度；该修正影响坐标，不改变朗读文本和分段策略。

迁移对照：同一语料再次同时调用原始 Kokoro API 和新的专用线程 KokoroBackend。五段样本数分别为 61200、100800、76800、104400、108000；各段最大 PCM 差值均为 0。该对照确认本机生成结果一致，仍不替代主观试听、设备边界和跨平台持续播放。
