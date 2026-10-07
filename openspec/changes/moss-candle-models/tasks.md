# Tasks

## 1. Resources and shared computation

- [x] 1.1 Pin official model/codec metadata and source licenses; verify resources by SHA-256 and record revisions in acceptance notes.
- [x] 1.2 Add the shared Candle MOSS computation crate and platform features; verify CPU and CUDA builds without a second Candle version.
- [x] 1.3 Implement Local global/depth token generation and official prompt semantics; verify real text-to-token EOS and a fixed upstream numerical reference.
- [x] 1.4 Implement native MOSS audio codec decode/encode and streaming state; verify reconstruction and generated WAV against official outputs.

## 2. Local 1.7B product integration

- [ ] 2.1 Verify Local Chinese WAV, valid termination, cancel/reuse, coverage and memory on RTX 5070; record metrics and listening material.
- [x] 2.2 Add verified Local model selection, resources and reusable voice cloning under moss; verify legacy Nano settings, isolated voices and source/checkpoint boundaries.
- [x] 2.3 Document Local commands and run worker/reader regression, Clippy, fmt and rustdoc for the changed feature combinations.

## 3. Other model modes

- [x] 3.1 Implement Realtime scheduling on the shared computation/codec and verify real Chinese output and cancel/reuse before catalog exposure.
- [x] 3.2 Implement VoiceGenerator delay scheduling and reusable designed references; verify a saved voice can generate a novel segment without redesign.
- [x] 3.3 Record mode-specific coverage, performance and manual listening status; verify publication includes only modes that passed real generation.
