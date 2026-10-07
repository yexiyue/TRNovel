> 已撤回（2026-10-07）：按用户决定移除 ZipVoice，CPU 默认仅保留 MOSS Nano。以下内容为历史方案，不再要求实现或验收。

# Why

Implement the complete official ONNX CPU pipeline using existing ORT, including text frontend, reference features, iterative flow matching and Vocos. Support INT8 and FP32 resources.

# What Changes

- Implement ZipVoice Distill CPU backend.
- Reuse verified downloads, PCM/End, cancellation, model ownership and existing settings.
- Record hardware and listening acceptance separately from compilation.

# Capabilities

## New Capabilities

- `integrate-zipvoice`: ZipVoice Distill CPU backend.

## Modified Capabilities

None.

# Impact

Listening worker, backend adapters, lightweight protocol and reader settings; inference dependencies remain outside the reader.
