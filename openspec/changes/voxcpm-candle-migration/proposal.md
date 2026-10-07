# Proposal

## Why

VoxCPM2 is the user's preferred listening model, but its worker currently ships a separate llama.cpp/ggml runtime. A native Candle implementation can share the workspace runtime while retaining the model, voices and real streaming behavior.

## What Changes

- Port complete VoxCPM2 inference and AudioVAE V2 to native Rust/Candle using the existing pinned Q8_0/F16 GGUF assets.
- Preserve worker protocol, model identity, devices and saved voices; version reference feature caches.
- Compare official numerical fixtures and current native baseline; require RTX 5070 RTF <= 0.8, first PCM <= 1.5s and 30-minute playback before replacing the production implementation.
- Remove Vox-specific C++/CMake after acceptance; retain evidence and explicitly outstanding platform/listening checks.

## Capabilities

### New Capabilities
- `voxcpm-candle-inference`: Native cancellable VoxCPM2 inference with reusable references, bounded streaming state and performance acceptance.

### Modified Capabilities
None.

## Impact

New computational crate, Vox worker adapter/features/cache/calibration, build packaging, numerical and real-model tools, documentation and acceptance notes. No reader native dependency or persisted model/voice identity change.

## Explicit staged integration decision (2026-10-07)

The user accepts the observed numerical differences and explicitly requests production Candle integration now. This supersedes the previous pre-integration gate: keep the strict F32 failure and all pending acceptance items visible without relaxing tolerances or declaring them passed. Native C++ remains a separate development-only benchmark crate until final qualification; production worker dependencies and features use only Candle. Human listening, 30-minute playback and external platforms remain independent acceptance work.
