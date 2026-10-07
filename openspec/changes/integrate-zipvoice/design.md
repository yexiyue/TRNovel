> 已撤回（2026-10-07）：按用户决定移除 ZipVoice，CPU 默认仅保留 MOSS Nano。以下内容为历史方案，不再要求实现或验收。

# Design

Implement the complete official ONNX CPU pipeline using existing ORT, including text frontend, reference features, iterative flow matching and Vocos. Support INT8 and FP32 resources.

## Decisions

- Native Rust/C++ only; no runtime Python.
- Model and voice identity includes backend, model ID and pinned revision.
- Existing configurations and checkpoint byte offsets remain valid.
- Explicit devices do not silently fall back; Auto uses measured calibration.
- No capability is advertised before its real inference path is validated.

## Validation

Windows RTX 5070 CUDA and CPU: valid PCM and completion, cancellation followed by a successful request, fixed Chinese corpus, memory and RTF measurements. GPU main tier requires RTF <= 0.8 and 30-minute continuity. Linux/Metal hardware acceptance remains explicit when hardware is absent.

## Implemented structure

- Pinned official INT8/FP32 encoder/decoder, Vocos and tokens size/SHA/license; existing ORT only.
- Complete Emilia Jieba/Pypinyin/Cn2An frontend and patched eSpeak English semantics, isolated native GPL helper.
- Eight-step flow matching with iterative cancellation, Rust mel/Vocos ISTFT and reference prompt caches.
- 17 official frontend comparison cases and Torch mel/ISTFT numeric fixture tests.
- Actual INT8/FP32 CPU PCM, cancellation/retry, matched Chinese ASR and shared model/voice integration.

## Acceptance boundaries

- Production release adapter model/voice cache and fixed corpus performance exports.
- Final quality checks, reader switching, external-platform builds and manual listening.

Evidence: `dev-notes/tts-model-tiers-acceptance.md`.
