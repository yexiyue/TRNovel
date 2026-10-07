# Design

## Context

See proposal.md. Current baseline uses fixed llama.cpp-omni 873056743b74e1a4ce5dcf7290e2298428e214db and existing Q8_0/F16 GGUF revision 169f64d8b98bbaab1761e4ca3a83e6af653456cc. Its full-corpus RTF is about .27. User accepts slower Candle inference if RTF <= .8.

## Goals / Non-Goals

Goals: native computational crate; faithful tokenization, MiniCPM/ResidualLM, FSQ, LocalEncoder/DiT/CFM and stateful AudioVAE V2; full voice modes; measured replacement.
Non-goals: Burn, private Candle forks, new public backend/engine IDs, new weight downloads, unrelated backend refactors.

### Authorized precision/performance follow-up (2026-10-07)

The user now authorizes evaluating original OpenBMB Safetensors alongside GGUF.
This supersedes the original no-new-download constraint for this evaluation.
Pin official weights to 32279effe8c19989596f05d353d1447f51d9e915, preserve
original names/precision and load AudioVAE weight normalization in Rust/F32.
Compare BF16 and F16 network execution with the existing Q8/F16 path serially.
Keep production model IDs/defaults unchanged until real inference qualifies the
new candidate. Cache one CFM timestep schedule per loaded model and project
conditioning once per patch, retaining sampling settings and cancellation.

## Decisions

- Official model semantics pinned to f0c787f0937dc1c9a8f4f64d9a332d9c5da2e629; GGUF names/layout cross-checked against current native code. Keep Apache/MIT attribution.
- Read existing GGUF directly; use Candle Q8 matmul and device-appropriate acoustic precision. CPU F32; GPU F16 with sensitive F32 calculations. Do not repeatedly dequantize weights or move activations to CPU.
- Shared model lifecycle, reference encoding, generation options and explicit outcome. Use bounded KV/causal convolution and transposed-convolution overlap state; do not decode full latent history per PCM chunk.
- Worker keeps existing thread ownership and bounded channel semantics, initializes owner before awaiting readiness. Check cancellation throughout inference, reference encoding and design.
- Version cached reference features with implementation, weights and source WAV digest. Preserve WAV/text and model identity.
- Save baseline executables/digests before migration. Compare serial release runs with seed42/10 CFM steps/CFG2/temperature1/max200; one warmup and five measurements. Separate wall/compute/backpressure, load/reference/first PCM and cancellation/release.

## Risks / Trade-offs

GGUF quantization and differing RNG can diverge waveforms → fixed activations/noise in numerical fixtures, report F16/Q8 errors separately.
Streaming convolution boundaries can introduce clicks → full-vs-chunked decoding tests including tails/reset.
Candle launch overhead may miss throughput → module profiling then local optimizations; do not reduce sampling quality to pass.
No external GPU host → report Linux/Metal build and hardware boundaries explicitly.

## Migration Plan

Implement and validate standalone crate first, then worker candidate and real acceptance. Only after gates pass replace existing feature wiring and remove Vox-specific C++/CMake. Keep independent baseline in ignored target directory. Preserve incomplete listening/platform tasks.

## Explicit staged integration decision (2026-10-07)

The user accepts the observed numerical differences and explicitly requests production Candle integration now. This supersedes the previous pre-integration gate: keep the strict F32 failure and all pending acceptance items visible without relaxing tolerances or declaring them passed. Native C++ remains a separate development-only benchmark crate until final qualification; production worker dependencies and features use only Candle. Human listening, 30-minute playback and external platforms remain independent acceptance work.
