# Proposal

## Why

AMD discrete and integrated GPU users need a Windows acceleration path. A small integrated GPU's performance must not determine support for all AMD hardware.

## What Changes

- Add an opt-in Rust development probe for the existing MOSS Nano ONNX pipeline with ORT DirectML.
- Enumerate DXGI adapters, require explicit selection and record LUID, vendor, memory and D3D12 capability.
- Reuse inference semantics while comparing CPU, DirectML host outputs and device-resident cache outputs; save valid EOS WAVs, timings, cancellation/reuse and provider profiles.
- Keep product devices/defaults unchanged until real hardware and model coverage are qualified.

## Capabilities

### New Capabilities
- `tts-directml-evaluation`: reproducible adapter-specific native TTS acceleration evidence.

### Modified Capabilities
None.

## Impact

Development-only backend feature/example, internal MOSS session construction and output placement, Windows DXGI dependency, acceptance report. No protocol or persisted configuration changes; no Python runtime or model downloads.
