# Tasks

## Implemented and verified

- [x] 1.1 Pinned official INT8/FP32 encoder/decoder, Vocos and tokens size/SHA/license; existing ORT only.
- [x] 1.2 Complete Emilia Jieba/Pypinyin/Cn2An frontend and patched eSpeak English semantics, isolated native GPL helper.
- [x] 1.3 Eight-step flow matching with iterative cancellation, Rust mel/Vocos ISTFT and reference prompt caches.
- [x] 1.4 17 official frontend comparison cases and Torch mel/ISTFT numeric fixture tests.
- [x] 1.5 Actual INT8/FP32 CPU PCM, cancellation/retry, matched Chinese ASR and shared model/voice integration.

## Remaining acceptance

- [x] 2.1 Production release adapter model/voice cache and fixed corpus performance exports.
- [ ] 2.2 Final quality checks, reader switching, external-platform builds and manual listening.

Evidence and limitations: `dev-notes/tts-model-tiers-acceptance.md`. Manual listening and unavailable-platform checks remain open.

- [x] Windows final workspace tests, Clippy, fmt, full rustdoc, feature matrix, docs build and rendered settings / real-worker callback regression.
