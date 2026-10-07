# Tasks

- [x] 1. Import the pinned upstream inference library with attribution and local workspace dependencies.
- [x] 2. Remove training-only C++ acceleration, upstream download/CLI code and custom PTX; use standard Candle operations.
- [x] 3. Wire independent qwen-cuda and ort-cuda features, explicit CUDA device construction and existing calibration/recovery.
- [x] 4. Update documentation and CI feature sets; pass portable workspace regression, formatting, Clippy and rustdoc.
- [ ] 5. Build the CUDA worker with a Toolkit and verify real GPU PCM/EOS, cancellation, performance and listening quality.
