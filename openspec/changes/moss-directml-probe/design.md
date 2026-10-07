# Design

## Context

ORT rc.13 has DirectML and CUDA13/DirectML joint native distributions. Current Nano graphs use FLOAT weights and opset17, with dynamic prefill/decode/codec dimensions. Only the CUDA run path currently binds persistent caches on-device. The local Ryzen 9700X has a small iGPU; AMD discrete GPU results remain separate.

## Goals / Non-Goals

Produce a native, repeatable hardware-specific probe. Do not change reader devices, Auto defaults, model exports or Candle backends. Windows AMD discrete GPUs are eligible by capability, not by the local iGPU result.

## Decisions

Use an optional `directml-probe` feature and DXGI enumeration. Require an adapter index from the same enumeration as DirectML; record LUID and vendor to make the selection reviewable and reject software or non-D3D12 adapters. CPU runs need no adapter. Reuse the Nano tokenizer, voice, generation, cancellation and PCM/EOS rules through an internal session factory. Disable memory patterns and parallel session execution for DML. No session is run concurrently.

Measure CPU, DML normal Run (host outputs), and DML I/O Binding for persistent caches independently. Record actual output allocation identity before claiming GPU cache residency. Save native ORT profiles separately from unprofiled timing so instrumentation does not distort throughput. Unsupported nodes can execute on CPU but their providers must be reported; registration is not acceleration evidence. Explicit failures terminate the probe without switching providers.

Warm up and then record five fixed-seed rounds on identical text/voice. Check valid nonempty finite PCM and EOS; dropping the bounded receiver cancels generation, followed by a successful new request. Initialization failures get a saved error report. Publish per-adapter results, reproducible commands and external hardware limits. Promote to product only in a separate qualified change.

## Risks / Trade-offs

Dynamic shapes can split DML graphs, and caches may not be fully GPU resident even with requested output placement. Device allocations crossing sessions require real validation. The local iGPU may be slower than CPU; this does not reject AMD discrete GPUs. Profiling and DirectML compilation can increase startup/RSS significantly. DLL selection must use the workspace's matched ORT runtime rather than replacing a provider alone.

## Open Questions

Runtime compatibility, cache allocation and performance are probe outcomes rather than product decisions. AMD discrete GPU throughput requires separate hardware; retain it as unverified when unavailable.
