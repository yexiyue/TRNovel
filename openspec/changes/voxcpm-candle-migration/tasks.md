# Tasks

## 1. Baseline and loading
- [x] 1.1 Preserve native release executables/digests and run same-corpus baseline metrics; record resource/source identities and licenses.
- [x] 1.2 Add computational crate and GGUF/tokenizer loading; verify tensor shapes, token IDs and CPU/CUDA builds.

## 2. Computation and streaming
- [ ] 2.1 Port MiniCPM/ResidualLM, LocalEncoder, FSQ and LocalDiT/CFM; validate official F32 fixtures and fixed-noise real-weight outputs.
  - Implemented; strict F32 oracle has 31 values outside tolerance in Transformer paths. No tolerance relaxation. CFM/FSQ/codec oracles pass; see dev-notes/voxcpm-candle-acceptance.md.
- [x] 2.2 Port AudioVAE encoding and bounded streaming decoding; test full/chunked equality, tails, cancellation and reset; document standalone probe.
- [ ] 2.3 Verify real Chinese no-reference/clone/continuation/design generation, EOS and cancel/reuse on CUDA and CPU; save WAV/metrics.
  - CPU no-reference EOS and cancellation/error/truncation/reuse passed; CUDA no-reference, corpus continuation, design and clone paths generated EOS WAVs. CPU with-text continuation now also generated valid EOS PCM; CPU no-transcript clone/design and human listening remain acceptance work.

## 3. Product and performance
- [x] 3.1 Adapt worker/voice design/cache versioning while preserving model IDs/features; verify old voices, config, device errors, truncation/checkpoints and rendered model switching.
  - Production migration explicitly authorized despite 2.1 and implemented. Old WAV/records preserved; versioned/precision-scoped cache rebuild, CLI design/reuse, worker EOS/cancel and real model/voice protocol switches pass. Rendered experimental-model switching with the real 0.11 release worker passes; broader qualification remains tracked separately.
- [x] 3.2 Run warmup plus five serial release corpus/long-text measurements and profile bottlenecks; record all gates and comparison.
  - CUDA corpus RTF .514-.559, long paragraph .529-.535; native hot baseline .253-.256; synchronized CFM profile ~62%. No performance evidence substitutes for 2.1 or 3.3.
- [ ] 3.3 Run 30-minute actual playback and cancel/reuse with memory/underrun evidence; record listening and external-platform boundaries.
  - Two-minute actual playback: 26 segments, zero underruns, stop/reuse passed; warm RSS/whole-GPU samples recorded. This does not substitute for 30 minutes.
- [ ] 3.4 After successful gates remove Vox-specific C++/CMake and update packaging/docs; verify workspace tests, Clippy, fmt, rustdoc, feature matrix and reader isolation.
  - Worker/adapter dependencies and features now contain only Candle; native benchmark moved to standalone voxcpm-sys example. Packaging/docs and all Windows workspace quality checks pass (533 tests), debug/release worker rebuilt, CPU/empty-feature checks and reader isolation pass. Delete remaining standalone C++ reference only after final qualification.

## Explicit staged integration decision (2026-10-07)

The user accepts the observed numerical differences and explicitly requests production Candle integration now. This supersedes the previous pre-integration gate: keep the strict F32 failure and all pending acceptance items visible without relaxing tolerances or declaring them passed. Native C++ remains a separate development-only benchmark crate until final qualification; production worker dependencies and features use only Candle. Human listening, 30-minute playback and external platforms remain independent acceptance work.

## 4. Original weights and CFM optimization follow-up
- [x] 4.1 Add pinned original Safetensors/BF16 and FP16 comparison paths, Rust AudioVAE weight normalization, tokenizer/config validation and verified cached resources.
- [x] 4.2 Cache CFM timestep embeddings and per-patch conditioning without changing sampling; validate fixed-noise output, step changes and cancellation/reuse.
- [x] 4.3 Measure five serial GPU rounds for Q8/BF16/FP16 with matching corpus/voice/parameters, memory and output checks; document results and expose only qualified candidates.
  - Mean RTF Q8 .522435 / BF16 .500738 / FP16 .505754; all corpus EOS and cancel/reuse regressions pass. Original weights remain an explicit computational/benchmark candidate, not a reader catalog entry: strict original F32 still fails (397 values), listening/30-minute/platform gates remain incomplete. See acceptance report and round-5 transcription/WAV materials.

## 5. Explicit reader experiment authorization (2026-10-07)

The user requests an experimental reader model option despite the recorded numerical and listening gates. Expose original BF16 only as a clearly labelled CUDA experiment; keep Q8 defaults and existing choices. Preserve pending acceptance records, model-specific resources/voices/reference cache/calibration and stop-before-switch behavior.

- [x] 5.1 Add original BF16 resource/catalog/worker/voice integration, reader experiment label and regression coverage; verify cached weights are reused and real CUDA switch/cancel/reuse works after ORT/CUDA upgrade.
  - Existing original weights reused; Q8 CUDA/BF16 Auto playback and release-before-switch pass. BF16 15-second playback, zero underruns, stop/reuse and rendered TUI experiment selection pass on Candle 0.11 / CUDA13. Nano ORT CUDA remains blocked independently.


## 6. User-authorized Candle stable upgrade

The user authorizes upgrading the unified workspace Candle beyond the original 0.9.2 plan. Target crates.io stable 0.11.0 (source revision 31f35b147389700ed2a178ee66a91c3cc25cc80d), without a private fork or simultaneous Candle versions. This does not add AMD Windows compute support; that remains an ORT DirectML investigation.

- [x] 6.1 Upgrade core/NN/transformers together, adapt APIs and document platform/MSRV changes.
- [ ] 6.2 Complete workspace quality checks, Windows CUDA Q8/BF16/ORT playback/switch/cancel regressions and serial performance comparison; keep external platform and listening qualifications pending.
  - 537 tests, Clippy, fmt, rustdoc, release, CPU/empty-feature and dependency-isolation checks pass. Five serial rounds mean RTF: old Q8 .526516, new Q8 .517762, new BF16 .514199. CPU Q8 smoke/cancel/reuse, Qwen/Omni/MOSS 1.7B CUDA, Nano CPU and real TUI switches pass. Nano ORT CUDA fails because the downloaded provider lacks sm120 cubins/PTX; keep this task incomplete until a compatible matching native distribution is verified.
