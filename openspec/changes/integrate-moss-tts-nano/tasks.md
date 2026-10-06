# Tasks

## 1. Architecture and protocol

- [x] 1.1 Extract concrete adapters into novel-tts-backends and separate audio playback from Kokoro features.
- [x] 1.2 Introduce bounded audio streams, backend segmentation, explicit completion and cancellation-safe playback checkpoints.
- [x] 1.3 Upgrade protocol and wire capability-driven backend/voice selection through CLI, worker and TUI.

## 2. MOSS and resources

- [x] 2.1 Pin and checksum models; validate native CPU graph loading with ort rc.10.
- [x] 2.2 Port tokenization, prompt assembly, fixed sampling, KV cache and streaming codec; compare against official inference.
- [x] 2.3 Implement atomic WAV voice import/list/remove with resampling and revision checks.

## 3. Acceptance and documentation

- [x] 3.1 Cover segmentation, streaming errors, buffer bounds, cancellation, backend switching and voice resources with regression tests.
- [x] 3.2 Validate MOSS-only, Kokoro-only, both-backend and basic-reader builds; run workspace quality checks.
- [x] 3.3 Exercise real Chinese, English, mixed and long-text synthesis, voice import and record local timing/memory evidence.
- [x] 3.4 Verify TUI selections, playback state and basic-reader layout with VHS.
- [x] 3.5 Update CLI, architecture, resource and project knowledge documentation.
