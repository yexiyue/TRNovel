# Tasks

## Implemented and verified

- [x] 1.1 Shared protocol v5 model/style fields and model-aware catalog, defaults and old Qwen compatibility.
- [x] 1.2 Pinned 1.7B CustomVoice/Base/VoiceDesign resource manifests and isolated model/voice/calibration identity.
- [x] 1.3 Real 0.6B/1.7B CustomVoice CUDA PCM, EOS, cancellation and CPU generation.
- [x] 1.4 Base progressive clone with reference validation and cached prompt; VoiceDesign saved as Base voice.
- [x] 1.5 Model/voice/style reader settings and shared CLI list/import/remove/design.
- [x] 1.6 Release 1.7B CustomVoice RTF <= 0.8 and 30-minute actual playback, stable VRAM, cancel/retry.

## Remaining acceptance

- [x] 2.1 Release styled full-corpus comparison and physical cancellation/release measurement.
- [ ] 2.2 Final full workspace quality checks and actual reader model/voice switching.
- [ ] 2.3 Manual per-segment listening; Linux CUDA and macOS Metal CI build and hardware acceptance.

Evidence and limitations: `dev-notes/tts-model-tiers-acceptance.md`. Manual listening and unavailable-platform checks remain open.

- [x] Windows final workspace tests, Clippy, fmt, full rustdoc, feature matrix, docs build and rendered settings / real-worker callback regression.
