# Tasks

## Implemented and verified

- [x] 1.1 Fixed llama.cpp-omni source revision, MIT source attribution and Q8_0/F16 GGUF size/SHA/license.
- [x] 1.2 Narrow lifecycle/device/reference/generation/PCM/cancel C ABI, static platform build and Rust thread ownership.
- [x] 1.3 Actual CUDA native streaming, cloned reference, callback cancellation, truncation and retry.
- [x] 1.4 Actual CPU inference with KV/op offload disabled and CUDA compute buffer zero.
- [x] 1.5 Shared download/voice/model/checkpoint/session/reader integration.

## Remaining acceptance

- [x] 2.1 Production voice design and fixed full-corpus WAV/ASR comparison.
- [x] 2.2 30-minute release actual playback RTF/VRAM acceptance.
- [ ] 2.3 Final quality checks, actual reader switching, external-platform builds and manual listening.

Evidence and limitations: `dev-notes/tts-model-tiers-acceptance.md`. Manual listening and unavailable-platform checks remain open.

- [x] Windows final workspace tests, Clippy, fmt, full rustdoc, feature matrix, docs build and rendered settings / real-worker callback regression.
