# Design

Integrate model selection, Qwen 1.7B CustomVoice, Base cloning and VoiceDesign. Preserve existing user configurations and validate real CUDA generation before enabling a new default.

## Decisions

- Native Rust/C++ only; no runtime Python.
- Model and voice identity includes backend, model ID and pinned revision.
- Existing configurations and checkpoint byte offsets remain valid.
- Explicit devices do not silently fall back; Auto uses measured calibration.
- No capability is advertised before its real inference path is validated.

## Validation

Windows RTX 5070 CUDA and CPU: valid PCM and completion, cancellation followed by a successful request, fixed Chinese corpus, memory and RTF measurements. GPU main tier requires RTF <= 0.8 and 30-minute continuity. Linux/Metal hardware acceptance remains explicit when hardware is absent.

## Implemented structure

- Shared protocol v5 model/style fields and model-aware catalog, defaults and old Qwen compatibility.
- Pinned 1.7B CustomVoice/Base/VoiceDesign resource manifests and isolated model/voice/calibration identity.
- Real 0.6B/1.7B CustomVoice CUDA PCM, EOS, cancellation and CPU generation.
- Base progressive clone with reference validation and cached prompt; VoiceDesign saved as Base voice.
- Model/voice/style reader settings and shared CLI list/import/remove/design.
- Release 1.7B CustomVoice RTF <= 0.8 and 30-minute actual playback, stable VRAM, cancel/retry.

## Acceptance boundaries

- Release styled full-corpus comparison and physical cancellation/release measurement.
- Final full workspace quality checks and actual reader model/voice switching.
- Manual per-segment listening; Linux CUDA and macOS Metal CI build and hardware acceptance.

Evidence: `dev-notes/tts-model-tiers-acceptance.md`.
