# Proposal

## Why

Qwen 的 Git 依赖阻碍本地维护，tokenizers 的训练加速在 Windows 引入与 ORT 冲突的 CRT；Qwen 本身尚未接入 CUDA。用户授权将小型上游库纳入 crates 并优先采用简洁架构，不保留旧 feature 兼容入口。

## What Changes

- 纳入固定 revision 的 MIT Qwen 推理库，统一 workspace 依赖与 Rust 模块规范。
- 推理库只计算，下载/校验、配置、校准和播放仍由既有层负责。
- 增加独立 Candle `qwen-cuda`，将 ORT CUDA 命名为 `ort-cuda`；保持平台特定 Metal。
- 移除分词器训练 C++ 加速以及独立 custom PTX/Flash Attention；残差归一化使用 Candle 算子。
- 显式设备选择不静默回退；复用 Auto 性能校准、PCM/EOS、取消和失败检查点。

## Capabilities

### Modified Capabilities

- `qwen-synthesis`: 本地推理库与 CPU/CUDA/Metal 设备边界。

## Impact

本地 crate、workspace、worker/backend features、CI/lefthook、构建脚本和说明。模型资源、协议及持久化格式不变。`cuda` 改名为 `ort-cuda` 是有意的构建接口变更。
