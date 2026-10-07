# Design

Adapt the pinned inference-only Rust implementation to workspace Candle without private forks, FlashAttention or runtime Python. Validate upstream parity and cancellable segmented generation.

## Decisions

- Native Rust/C++ only; no runtime Python.
- Model and voice identity includes backend, model ID and pinned revision.
- Existing configurations and checkpoint byte offsets remain valid.
- Explicit devices do not silently fall back; Auto uses measured calibration.
- No capability is advertised before its real inference path is validated.

## Validation

Windows RTX 5070 CUDA and CPU: valid PCM and completion, cancellation followed by a successful request, fixed Chinese corpus, memory and RTF measurements. GPU main tier requires RTF <= 0.8 and 30-minute continuity. Linux/Metal hardware acceptance remains explicit when hardware is absent.

## Implemented structure

- Inference-only vendoring at fixed revision, shared Candle 0.9.2; no fork/FlashAttention/Vulkan/WGPU/ASR/server.
- Fixed weight files size/SHA and explicit mixed tokenizer license.
- Actual CUDA clone/design and CPU inference; supported tag descriptions and cached clone prompts.
- Semantic segment PCM with native_streaming=false; direct channel cancellation at diffusion/decode boundaries.
- Shared download/voice/model/reader/worker integration.

## Acceptance boundaries

- Verify synchronous cancellation plus backend release and subsequent successful generation after final change.
- Production voice design and full-corpus/ASR comparison.
- 30-minute release actual playback RTF/VRAM acceptance.
- Final quality checks, reader switching, external-platform builds and manual listening.

Evidence: `dev-notes/tts-model-tiers-acceptance.md`.
