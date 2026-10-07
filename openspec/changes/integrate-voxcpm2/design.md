# Design

Integrate the pinned llama.cpp-omni VoxCPM2 implementation through a narrow cancellable C ABI. Validate real Windows CUDA inference before publishing capabilities.

## Decisions

- Native Rust/C++ only; no runtime Python.
- Model and voice identity includes backend, model ID and pinned revision.
- Existing configurations and checkpoint byte offsets remain valid.
- Explicit devices do not silently fall back; Auto uses measured calibration.
- No capability is advertised before its real inference path is validated.

## Validation

Windows RTX 5070 CUDA and CPU: valid PCM and completion, cancellation followed by a successful request, fixed Chinese corpus, memory and RTF measurements. GPU main tier requires RTF <= 0.8 and 30-minute continuity. Linux/Metal hardware acceptance remains explicit when hardware is absent.

## Implemented structure

- Fixed llama.cpp-omni source revision, MIT source attribution and Q8_0/F16 GGUF size/SHA/license.
- Narrow lifecycle/device/reference/generation/PCM/cancel C ABI, static platform build and Rust thread ownership.
- Actual CUDA native streaming, cloned reference, callback cancellation, truncation and retry.
- Actual CPU inference with KV/op offload disabled and CUDA compute buffer zero.
- Shared download/voice/model/checkpoint/session/reader integration.

## Acceptance boundaries

- Production voice design and fixed full-corpus WAV/ASR comparison.
- 30-minute release actual playback RTF/VRAM acceptance.
- Final quality checks, actual reader switching, external-platform builds and manual listening.

Evidence: `dev-notes/tts-model-tiers-acceptance.md`.
