# Tasks

## 1. Native probe
- [x] 1.1 Add optional feature and DXGI enumeration; verify software/invalid selection rejects and list shows actual hardware identity.
- [x] 1.2 Reuse Nano sessions/generation with host/device cache modes and profiling; verify CPU regression and native probe build.
- [x] 1.3 Save finite EOS WAVs, five rounds, cancel/reuse and error evidence; document reproducible commands.

## 2. Hardware evidence
- [x] 2.1 Run local CPU and AMD DirectML serially, inspect provider profiles/cache allocation and record failures or timings.
- [ ] 2.2 Run on AMD discrete hardware and record adapter-specific results; retain unverified if unavailable.

Local hardware is the Ryzen 9700X iGPU. No physical AMD discrete GPU is available;
task 2.2 remains unverified. Timing, profiles, ASR and remaining qualification are
recorded in `dev-notes/moss-directml-evaluation.md`.

## 3. Integration checks
- [x] 3.1 Run relevant tests, Clippy, fmt, rustdoc and feature checks; verify reader/device/config behavior unchanged and document limitations.
