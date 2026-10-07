# Tasks

## Implemented and verified

- [x] 1.1 Inference-only vendoring at fixed revision, shared Candle 0.9.2; no fork/FlashAttention/Vulkan/WGPU/ASR/server.
- [x] 1.2 Fixed weight files size/SHA and explicit mixed tokenizer license.
- [x] 1.3 Actual CUDA clone/design and CPU inference; supported tag descriptions and cached clone prompts.
- [x] 1.4 Semantic segment PCM with native_streaming=false; direct channel cancellation at diffusion/decode boundaries.
- [x] 1.5 Shared download/voice/model/reader/worker integration.

## Remaining acceptance

- [x] 2.1 Verify synchronous cancellation plus backend release and subsequent successful generation after final change.
- [x] 2.2 Production voice design and full-corpus/ASR comparison.
- [ ] 2.3 30-minute release actual playback RTF/VRAM acceptance.
- [ ] 2.4 Final quality checks, reader switching, external-platform builds and manual listening.

Evidence and limitations: `dev-notes/tts-model-tiers-acceptance.md`. Manual listening and unavailable-platform checks remain open.

- [x] Windows final workspace tests, Clippy, fmt, full rustdoc, feature matrix, docs build and rendered settings / real-worker callback regression.
- User stopped playback after 1561 seconds / 330 segments without failure, RTF .272 and stable VRAM; full 30-minute item remains open. Separate early cancellation/retry and release probes passed.
