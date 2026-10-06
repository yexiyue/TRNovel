# Tasks

## 1. Compatibility gate

- [x] 1.1 Validate pinned Qwen Q4 / float graph loading and real inference with ort rc.10; record provider availability and reference precision before production integration.

## 2. Continuous playback

- [x] 2.1 Add source mapping, voice duration estimator and contextual speech blocks.
- [x] 2.2 Add bounded streaming boundary-silence processing and shared PCM.
- [x] 2.3 Implement continuous playback markers and sample position across pause / speed / cancellation.

## 3. Alignment

- [x] 3.1 Introduce model-independent aligner and sentence timeline; implement native Qwen preprocessing / tokenizer / timestamp decoding.
- [x] 3.2 Integrate bounded asynchronous alignment, monotonic highlighting and played-sentence checkpoints with fallback.
- [x] 3.3 Upgrade protocol v3, CLI and TUI for alignment and execution-device status.

## 4. Acceleration

- [x] 4.1 Add CoreML / CUDA features, native provider packaging and independent component selection.
- [x] 4.2 Implement pinned variant resources, 3+5 end-to-end calibration cache, concurrent-load validation and explicit fallback.
- [ ] 4.3 Validate GPU tensor / KV transfer behavior and provider partition evidence.

## 5. Acceptance

- [ ] 5.1 Validate reference / manually annotated alignment and audio continuity, recording latency / throughput / memory.
- [x] 5.2 Run feature combinations, full quality checks and VHS; update architecture / resources / acceptance docs.

## Evidence and remaining acceptance

See `dev-notes/continuous-tts-acceptance.md` for reproducible commands, reference results, M4 Pro release measurements and VHS results.

- 4.3: M4 Pro CoreML graph loading, inference, PCM parity and provider partition evidence recorded. CUDA I/O binding is implemented; NVIDIA hardware transfer/profile and packaged runtime execution remain unverified.
- 5.1: Chinese official reference parity, native English/mixed inference, same-text audio comparison and component timing/memory measurements recorded. Independent manual boundary annotations, hardware audio gap capture and full worker peak memory remain outstanding.
- Accelerated release workflow and native library packaging are implemented; cross-platform workflow execution and artifact runtime acceptance remain outstanding.
