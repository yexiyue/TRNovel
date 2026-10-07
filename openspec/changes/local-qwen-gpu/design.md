# Design

## Decisions

模型库拥有 Candle 和设备创建；适配器只翻译 protocol Device 和 PCM，worker 保留已有生命周期职责。GPU feature 明确区分 Candle CUDA 与 ORT CUDA，不维护兼容别名。模型库的 cuda/metal 按平台和 SDK 显式构建，普通 CI 选择 portable feature 集，GPU SDK lane 做 CUDA 编译检查。

禁用 tokenizers 默认训练加速，只保留 onig 推理；源头解决 esaxx 的静态 CRT 冲突。Candle 自带算子实现 residual normalization，不额外管理手写 PTX。现有 CUDA preallocated KV cache 保留。校准实现标识变为 qwen-local-v1。

## Validation

检查 Qwen CPU 单独构建及与 ORT CUDA 共存的链接、推理库/适配器回归、workspace lint/fmt/docs、阅读器依赖隔离。GPU 编译需要 nvcc，真实 EOS/PCM、首包延迟、RTF、显存和取消必须用真实 GPU/模型验证；SDK 或模型缺失时保留对应未完成任务。
